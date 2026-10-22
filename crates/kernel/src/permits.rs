//! Permit model and resolver.
//!
//! Resolution order is fixed: owner bypass, then manager (Administrator/ManageGuild)
//! bypass, then `enforced` on the user target, then `custom` buckets in chain order
//! (user, channel, roles high→low). Deny wins inside a bucket, and the first tier
//! that returns a verdict decides; a tier with no matching node passes to the next.

use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

pub use lumi_db::entities::enums::{PermitKind, PermitPolarity, PermitTargetType};
use lumi_db::entities::{permit, permit_assignment};

use crate::Result;

/// Permit definition row.
#[derive(Clone, Debug)]
pub struct PermitRow {
    /// Database identifier.
    pub id: i32,
    /// Guild snowflake ID.
    pub guild_id: String,
    /// Permit name.
    pub name: String,
    /// Permit kind.
    pub kind: PermitKind,
    /// Permit polarity.
    pub polarity: PermitPolarity,
    /// Target permission nodes.
    pub nodes: Vec<String>,
    /// Whether this is a builtin permit.
    pub builtin: bool,
    /// Sort position.
    pub position: i32,
}

impl PermitRow {
    fn from_model(model: permit::Model) -> Self {
        Self {
            id: model.id,
            guild_id: model.guild_id.to_string(),
            name: model.name,
            kind: model.kind,
            polarity: model.polarity,
            nodes: model.nodes,
            builtin: model.builtin,
            position: model.position,
        }
    }
}

/// Assignment linking a permit to a target.
#[derive(Clone, Debug)]
pub struct PermitAssignment {
    /// Associated permit ID.
    pub permit_id: i32,
    /// Guild snowflake ID.
    pub guild_id: String,
    /// Target type.
    pub target_type: PermitTargetType,
    /// Target snowflake ID.
    pub target_id: i64,
}

/// Scope of a permission node pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeScope {
    /// `*` — matches any node.
    Wildcard,
    /// A literal node, e.g. `mod.ban`.
    Exact,
    /// A namespace prefix, e.g. `admin.*`.
    Namespace,
}

impl NodeScope {
    /// Classify a permission node pattern.
    pub fn of(pattern: &str) -> Self {
        if pattern == "*" {
            Self::Wildcard
        } else if pattern.ends_with(".*") {
            Self::Namespace
        } else {
            Self::Exact
        }
    }
}

/// Check if a node string matches a pattern.
pub fn node_matches(pattern: &str, node: &str) -> bool {
    if pattern.is_empty() || node.is_empty() {
        return false;
    }
    match NodeScope::of(pattern) {
        NodeScope::Wildcard => true,
        NodeScope::Exact => pattern == node,
        NodeScope::Namespace => pattern.strip_suffix(".*").is_some_and(|namespace| {
            node == namespace
                || node
                    .strip_prefix(namespace)
                    .is_some_and(|rest| rest.starts_with('.'))
        }),
    }
}

/// Resolution verdict for a permit evaluation tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Access granted.
    Allow,
    /// Access denied.
    Deny,
    /// No matching rule found; evaluate next tier.
    Pass,
}

/// One row of the tier trace, mirroring lumi's `PermitTierExplanation`.
#[derive(Clone, Debug)]
pub struct TierExplanation {
    /// Name of evaluated tier.
    pub tier: String,
    /// Verdict produced by tier.
    pub verdict: Verdict,
    /// Optional detail.
    pub detail: Option<String>,
}

/// Final permission resolution decision and explanation trace.
#[derive(Clone, Debug)]
pub struct Decision {
    /// Whether access is allowed.
    pub allowed: bool,
    /// Evaluation trace rows.
    pub rows: Vec<TierExplanation>,
}

/// Context for evaluating permits on an invocation.
#[derive(Clone, Debug)]
pub struct CheckContext<'a> {
    /// Guild snowflake ID.
    pub guild_id: String,
    /// Invoker user ID.
    pub user_id: u64,
    /// Invocation channel ID.
    pub channel_id: Option<u64>,
    /// Role ids ordered high→low (Discord position desc).
    pub role_ids: &'a [u64],
    /// Whether invoker is bot owner.
    pub is_owner: bool,
    /// Whether invoker is server owner.
    pub is_guild_owner: bool,
    /// Whether invoker has Administrator or `ManageGuild`.
    pub is_manager: bool,
    /// Whether invoker is quarantined.
    pub is_quarantined: bool,
    /// Permission node being requested.
    pub node: &'a str,
}

/// Map an invoked command name to the permit node it requires.
/// Commands absent from this map are unpermitted and pass freely.
pub fn required_node(command: &str) -> Option<&'static str> {
    match command {
        "warn" => Some("mod.warn"),
        "ban" => Some("mod.ban"),
        "kick" => Some("mod.kick"),
        "mute" | "unmute" | "timeout" => Some("mod.mute"),
        "quarantine" => Some("mod.quarantine"),
        "purge" => Some("mod.purge"),
        "lumi" | "repo" | "download" => Some("admin.*"),
        _ => None,
    }
}

/// The permit node a command requires.
///
/// A `&'static str` placed in the command's poise `custom_data` wins, so a
/// command (or subcommand) can declare its own node; otherwise the built-in
/// [`required_node`] name map is used.
pub fn command_node(command: &crate::bot::Command) -> Option<&'static str> {
    command
        .custom_data
        .downcast_ref::<&'static str>()
        .copied()
        .or_else(|| required_node(&command.name))
}

/// One guild's permits and target index. Kept per guild so a permit edit only
/// reloads that guild instead of every permit in the fleet.
#[derive(Debug, Default)]
struct PermitIndex {
    permits: HashMap<i32, PermitRow>,
    by_user: HashMap<i64, Vec<i32>>,
    by_channel: HashMap<i64, Vec<i32>>,
    by_role: HashMap<i64, Vec<i32>>,
}

/// In-memory resolver evaluating guild permit hierarchies.
#[derive(Debug)]
pub struct PermitResolver {
    db: Option<DatabaseConnection>,
    guilds: DashMap<String, Arc<PermitIndex>>,
}

fn build_index(permits: Vec<PermitRow>, assignments: Vec<PermitAssignment>) -> PermitIndex {
    let permit_map = permits.into_iter().map(|p| (p.id, p)).collect();

    let mut by_user: HashMap<i64, Vec<i32>> = HashMap::new();
    let mut by_channel: HashMap<i64, Vec<i32>> = HashMap::new();
    let mut by_role: HashMap<i64, Vec<i32>> = HashMap::new();

    for a in assignments {
        match a.target_type {
            PermitTargetType::User => by_user.entry(a.target_id).or_default().push(a.permit_id),
            PermitTargetType::Channel => by_channel.entry(a.target_id).or_default().push(a.permit_id),
            PermitTargetType::Role => by_role.entry(a.target_id).or_default().push(a.permit_id),
        }
    }

    PermitIndex {
        permits: permit_map,
        by_user,
        by_channel,
        by_role,
    }
}

/// Load every permit and assignment once at startup.
///
/// Reloads after a mutation use [`load_guild_parts`] instead: reading the whole
/// table per dashboard toggle would grow with the number of customised guilds.
async fn load_parts(db: &DatabaseConnection) -> Result<(Vec<PermitRow>, Vec<PermitAssignment>)> {
    let permits = permit::Entity::find()
        .all(db)
        .await?
        .into_iter()
        .map(PermitRow::from_model)
        .collect();
    let assignments = permit_assignment::Entity::find()
        .all(db)
        .await?
        .into_iter()
        .map(|a| PermitAssignment {
            permit_id: a.permit_id,
            guild_id: a.guild_id.to_string(),
            target_type: a.target_type,
            target_id: a.target_id.as_i64(),
        })
        .collect();
    Ok((permits, assignments))
}

/// Load one guild's permits and assignments.
async fn load_guild_parts(
    db: &DatabaseConnection,
    guild_id: u64,
) -> Result<(Vec<PermitRow>, Vec<PermitAssignment>)> {
    let guild = lumi_db::ids::GuildId::from(guild_id);
    let permits = permit::Entity::find()
        .filter(permit::Column::GuildId.eq(guild))
        .all(db)
        .await?
        .into_iter()
        .map(PermitRow::from_model)
        .collect();
    let assignments = permit_assignment::Entity::find()
        .filter(permit_assignment::Column::GuildId.eq(guild))
        .all(db)
        .await?
        .into_iter()
        .map(|a| PermitAssignment {
            permit_id: a.permit_id,
            guild_id: a.guild_id.to_string(),
            target_type: a.target_type,
            target_id: a.target_id.as_i64(),
        })
        .collect();
    Ok((permits, assignments))
}

fn tier_row(tier: &str, verdict: Verdict, detail: Option<String>) -> TierExplanation {
    TierExplanation {
        tier: tier.into(),
        verdict,
        detail,
    }
}

/// Bucket key: numeric target id.
fn target_id(id: u64) -> i64 {
    i64::try_from(id).unwrap_or(i64::MAX)
}

impl PermitResolver {
    /// Construct a resolver configured for lazy per-guild loading.
    #[must_use]
    pub fn load(db: &DatabaseConnection) -> Self {
        Self {
            db: Some(db.clone()),
            guilds: DashMap::new(),
        }
    }

    /// Load every permit and assignment across all guilds at once.
    pub async fn load_all(db: &DatabaseConnection) -> Result<Self> {
        let (permits, assignments) = load_parts(db).await?;
        let mut resolver = Self::new(permits, assignments);
        resolver.db = Some(db.clone());
        Ok(resolver)
    }

    /// Ensure one guild's permits are loaded in cache, reading from the database on miss.
    pub async fn ensure_guild(&self, db: &DatabaseConnection, guild_id: u64) -> Result<()> {
        let key = guild_id.to_string();
        if self.guilds.contains_key(&key) {
            return Ok(());
        }
        let (permits, assignments) = load_guild_parts(db, guild_id).await?;
        self.guilds
            .entry(key)
            .or_insert_with(|| Arc::new(build_index(permits, assignments)));
        Ok(())
    }

    /// Ensure one guild's permits are loaded using the database handle configured on the resolver.
    pub async fn ensure_guild_loaded(&self, guild_id: u64) -> Result<()> {
        let key = guild_id.to_string();
        if self.guilds.contains_key(&key) {
            return Ok(());
        }
        if let Some(ref db) = self.db {
            let (permits, assignments) = load_guild_parts(db, guild_id).await?;
            self.guilds
                .entry(key)
                .or_insert_with(|| Arc::new(build_index(permits, assignments)));
        }
        Ok(())
    }

    /// Replace one guild's index after a permit/assignment write.
    pub async fn reload_guild(&self, db: &DatabaseConnection, guild_id: u64) -> Result<()> {
        let (permits, assignments) = load_guild_parts(db, guild_id).await?;
        self.guilds.insert(
            guild_id.to_string(),
            Arc::new(build_index(permits, assignments)),
        );
        Ok(())
    }

    /// Create a new resolver seeded with permit rows and assignments.
    pub fn new(permits: Vec<PermitRow>, assignments: Vec<PermitAssignment>) -> Self {
        let mut by_guild: HashMap<String, (Vec<PermitRow>, Vec<PermitAssignment>)> = HashMap::new();
        for permit in permits {
            by_guild
                .entry(permit.guild_id.clone())
                .or_default()
                .0
                .push(permit);
        }
        for assignment in assignments {
            by_guild
                .entry(assignment.guild_id.clone())
                .or_default()
                .1
                .push(assignment);
        }

        let guilds = DashMap::new();
        for (guild_id, (permits, assignments)) in by_guild {
            guilds.insert(guild_id, Arc::new(build_index(permits, assignments)));
        }
        Self { db: None, guilds }
    }

    /// Decide access for one node; the returned rows stop at the deciding tier.
    pub fn resolve(&self, ctx: &CheckContext<'_>) -> Decision {
        let rows = self.explain(ctx);
        let allowed = rows
            .iter()
            .rev()
            .find(|row| row.verdict != Verdict::Pass)
            .is_some_and(|row| row.verdict == Verdict::Allow);
        Decision { allowed, rows }
    }

    /// Every tier evaluated for one node, in resolution order, including
    /// passing tiers. Stops at the first deciding tier.
    pub fn explain(&self, ctx: &CheckContext<'_>) -> Vec<TierExplanation> {
        match self.guilds.get(&ctx.guild_id) {
            Some(index) => index.explain(ctx),
            None => PermitIndex::default().explain(ctx),
        }
    }
}

impl PermitIndex {
    /// Every tier evaluated for one node, in resolution order, including
    /// passing tiers. Stops at the first deciding tier.
    fn explain(&self, ctx: &CheckContext<'_>) -> Vec<TierExplanation> {
        let mut rows = Vec::new();

        if ctx.is_owner {
            rows.push(tier_row("owner", Verdict::Allow, Some("bot owner".into())));
            return rows;
        }
        if ctx.is_guild_owner {
            rows.push(tier_row(
                "owner",
                Verdict::Allow,
                Some("server owner".into()),
            ));
            return rows;
        }
        rows.push(tier_row("owner", Verdict::Pass, None));

        if ctx.is_manager {
            rows.push(tier_row(
                "manager",
                Verdict::Allow,
                Some("Administrator or ManageGuild".into()),
            ));
            return rows;
        }
        rows.push(tier_row("manager", Verdict::Pass, None));

        let mut decided = false;
        let mut allowed = false;
        let user = target_id(ctx.user_id);

        consider(
            &mut rows,
            &mut decided,
            &mut allowed,
            self.evaluate_bucket("enforced:user", self.by_user.get(&user).map(Vec::as_slice), ctx.node, true),
        );
        if decided || ctx.is_quarantined {
            if ctx.is_quarantined && !decided {
                rows.push(tier_row(
                    "quarantine",
                    Verdict::Deny,
                    Some("user is quarantined".into()),
                ));
            }
            return rows;
        }

        consider(
            &mut rows,
            &mut decided,
            &mut allowed,
            self.evaluate_bucket("custom:user", self.by_user.get(&user).map(Vec::as_slice), ctx.node, false),
        );
        if decided {
            return rows;
        }

        if let Some(channel_id) = ctx.channel_id {
            let channel = target_id(channel_id);
            consider(
                &mut rows,
                &mut decided,
                &mut allowed,
                self.evaluate_bucket(
                    "custom:channel",
                    self.by_channel.get(&channel).map(Vec::as_slice),
                    ctx.node,
                    false,
                ),
            );
            if decided {
                return rows;
            }
        }

        for &role_id in ctx.role_ids {
            let role = target_id(role_id);
            consider(
                &mut rows,
                &mut decided,
                &mut allowed,
                self.evaluate_bucket(
                    &format!("custom:role:{role_id}"),
                    self.by_role.get(&role).map(Vec::as_slice),
                    ctx.node,
                    false,
                ),
            );
            if decided {
                return rows;
            }
        }

        rows
    }

    /// Deny wins inside one bucket, then grant, else the tier passes.
    fn evaluate_bucket(
        &self,
        tier: &str,
        permit_ids: Option<&[i32]>,
        node: &str,
        enforced_only: bool,
    ) -> TierExplanation {
        let Some(ids) = permit_ids else {
            return tier_row(tier, Verdict::Pass, None);
        };

        let wanted = if enforced_only {
            PermitKind::Enforced
        } else {
            PermitKind::Custom
        };
        let mut deny = None;
        let mut grant = None;

        for &id in ids {
            let Some(permit) = self.permits.get(&id) else {
                continue;
            };
            if permit.kind != wanted {
                continue;
            }
            if let Some(pattern) = permit
                .nodes
                .iter()
                .find(|pattern| node_matches(pattern, node))
            {
                match permit.polarity {
                    PermitPolarity::Deny => deny = Some(pattern.clone()),
                    PermitPolarity::Grant => {
                        if grant.is_none() {
                            grant = Some(pattern.clone());
                        }
                    }
                }
            }
        }

        match (deny, grant) {
            (Some(pattern), _) => tier_row(tier, Verdict::Deny, Some(format!("deny {pattern}"))),
            (None, Some(pattern)) => {
                tier_row(tier, Verdict::Allow, Some(format!("grant {pattern}")))
            }
            (None, None) => tier_row(tier, Verdict::Pass, None),
        }
    }
}

/// Subject context for permit explanation.
#[derive(Clone, Debug)]
pub struct ExplainSubject {
    /// Discord user snowflake.
    pub user_id: u64,
    /// Channel snowflake if channel-scoped.
    pub channel_id: Option<u64>,
    /// Discord role snowflakes.
    pub role_ids: Vec<u64>,
    /// Whether user is a bot owner.
    pub is_owner: bool,
    /// Whether user is the guild owner.
    pub is_guild_owner: bool,
    /// Whether user has manager permissions.
    pub is_manager: bool,
}

/// Walk every tier that decides `node` for `subject`, mirroring the TS
/// `explainPermit`. The first deciding tier wins.
pub async fn explain_permit(
    db: &DatabaseConnection,
    guild_id: u64,
    node: &str,
    subject: &ExplainSubject,
) -> Result<Vec<TierExplanation>> {
    let (permits, assignments) = load_guild_parts(db, guild_id).await?;
    let resolver = PermitResolver::new(permits, assignments);
    let ctx = CheckContext {
        guild_id: guild_id.to_string(),
        user_id: subject.user_id,
        channel_id: subject.channel_id,
        role_ids: &subject.role_ids,
        is_owner: subject.is_owner,
        is_guild_owner: subject.is_guild_owner,
        is_manager: subject.is_manager,
        is_quarantined: false,
        node,
    };
    Ok(resolver.explain(&ctx))
}

fn consider(
    rows: &mut Vec<TierExplanation>,
    decided: &mut bool,
    allowed: &mut bool,
    row: TierExplanation,
) {
    if *decided {
        return;
    }
    match row.verdict {
        Verdict::Allow => {
            *decided = true;
            *allowed = true;
            rows.push(row);
        }
        Verdict::Deny => {
            *decided = true;
            *allowed = false;
            rows.push(row);
        }
        Verdict::Pass => {
            rows.push(row);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn permit(id: i32, kind: PermitKind, polarity: PermitPolarity, nodes: &[&str]) -> PermitRow {
        PermitRow {
            id,
            guild_id: "1".into(),
            name: format!("p{id}"),
            kind,
            polarity,
            nodes: nodes.iter().map(ToString::to_string).collect(),
            builtin: false,
            position: id,
        }
    }

    fn assignment(
        permit_id: i32,
        target_type: PermitTargetType,
        target_id: i64,
    ) -> PermitAssignment {
        PermitAssignment {
            permit_id,
            guild_id: "1".into(),
            target_type,
            target_id,
        }
    }

    fn ctx<'a>(role_ids: &'a [u64], node: &'a str) -> CheckContext<'a> {
        CheckContext {
            guild_id: "1".into(),
            user_id: 7,
            channel_id: None,
            role_ids,
            is_owner: false,
            is_guild_owner: false,
            is_manager: false,
            is_quarantined: false,
            node,
        }
    }

    #[test]
    fn node_patterns() {
        assert!(node_matches("*", "mod.ban"));
        assert!(node_matches("mod.ban", "mod.ban"));
        assert!(!node_matches("mod.ban", "mod.kick"));
        assert!(node_matches("admin.*", "admin.b"));
        assert!(node_matches("admin.*", "admin"));
        assert!(!node_matches("admin.*", "administrator"));
        assert!(!node_matches("", "mod.ban"));
        assert!(!node_matches("mod.ban", ""));
    }

    #[test]
    fn owner_always_allowed() {
        let resolver = PermitResolver::new(Vec::new(), Vec::new());
        let mut c = ctx(&[], "mod.ban");
        c.is_owner = true;
        assert!(resolver.resolve(&c).allowed);
    }

    #[test]
    fn command_node_reads_custom_data_then_name() {
        let mut command = crate::bot::Command {
            name: "ban".into(),
            ..Default::default()
        };
        assert_eq!(command_node(&command), Some("mod.ban"));

        command.custom_data = Box::new("custom.node");
        assert_eq!(command_node(&command), Some("custom.node"));

        assert_eq!(required_node("tempvc"), None);
        assert_eq!(required_node("panel"), None);
        assert_eq!(required_node("lumi"), Some("admin.*"));
    }

    #[test]
    fn user_custom_grant_decides() {
        let permits = vec![permit(
            1,
            PermitKind::Custom,
            PermitPolarity::Grant,
            &["mod.ban"],
        )];
        let resolver = PermitResolver::new(permits, vec![assignment(1, PermitTargetType::User, 7)]);
        let decision = resolver.resolve(&ctx(&[], "mod.ban"));
        assert!(decision.allowed);
        assert!(decision.rows.iter().any(|r| r.tier == "custom:user"));
    }

    #[test]
    fn guild_indexes_are_isolated() {
        let permits = vec![permit(
            1,
            PermitKind::Custom,
            PermitPolarity::Grant,
            &["mod.ban"],
        )];
        let resolver = PermitResolver::new(permits, vec![assignment(1, PermitTargetType::User, 7)]);
        assert!(resolver.resolve(&ctx(&[], "mod.ban")).allowed);

        let mut other_guild = ctx(&[], "mod.ban");
        other_guild.guild_id = "2".into();
        assert!(!resolver.resolve(&other_guild).allowed);
    }

    #[test]
    fn enforced_deny_beats_custom_grant() {
        let permits = vec![
            permit(1, PermitKind::Enforced, PermitPolarity::Deny, &["*"]),
            permit(2, PermitKind::Custom, PermitPolarity::Grant, &["*"]),
        ];
        let resolver = PermitResolver::new(
            permits,
            vec![
                assignment(1, PermitTargetType::User, 7),
                assignment(2, PermitTargetType::User, 7),
            ],
        );
        let decision = resolver.resolve(&ctx(&[], "mod.ban"));
        assert!(!decision.allowed);
        assert_eq!(
            decision.rows.last().map(|r| r.tier.as_str()),
            Some("enforced:user")
        );
    }

    #[test]
    fn quarantine_denies_when_undecided() {
        let resolver = PermitResolver::new(Vec::new(), Vec::new());
        let mut c = ctx(&[], "mod.ban");
        c.is_quarantined = true;
        let decision = resolver.resolve(&c);
        assert!(!decision.allowed);
        assert!(decision.rows.iter().any(|r| r.tier == "quarantine"));
    }

    #[test]
    fn explain_lists_passing_tiers() {
        let resolver = PermitResolver::new(Vec::new(), Vec::new());
        let rows = resolver.explain(&ctx(&[], "mod.ban"));
        assert_eq!(rows.first().map(|r| r.tier.as_str()), Some("owner"));
        assert!(
            rows.iter()
                .any(|r| r.tier == "manager" && r.verdict == Verdict::Pass)
        );
    }
}
