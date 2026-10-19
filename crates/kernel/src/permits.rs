//! Permit model and resolver, ported from lumi's `api/permissions/PermitResolver.ts`.
//!
//! Resolution order is fixed and mirrors the TS: owner bypass, then manager
//! (Administrator/ManageGuild) bypass, then `enforced` on the user target, then
//! `custom` buckets in chain order (user, channel, roles high→low). Deny wins
//! inside a bucket, and the first tier that returns a verdict decides; a tier
//! with no matching node passes to the next.
//!
//! One engine produces both the fast decision ([`PermitResolver::resolve`]) and
//! the full tier walk ([`PermitResolver::explain`], [`explain_permit`]).

use std::collections::HashMap;

use sea_orm::{DatabaseConnection, EntityTrait};

pub use lumi_db::entities::enums::{PermitKind, PermitPolarity, PermitTargetType};
use lumi_db::entities::{permit, permit_assignment};

use crate::Result;

/// A permit as loaded from the `permits` table.
#[derive(Clone, Debug)]
pub struct PermitRow {
    pub id: i32,
    pub guild_id: String,
    pub name: String,
    pub kind: PermitKind,
    pub polarity: PermitPolarity,
    pub nodes: Vec<String>,
    pub builtin: bool,
    pub position: i32,
}

impl PermitRow {
    fn from_model(model: permit::Model) -> Self {
        Self {
            id: model.id,
            guild_id: model.guild_id,
            name: model.name,
            kind: model.kind,
            polarity: model.polarity,
            nodes: model.nodes,
            builtin: model.builtin,
            position: model.position,
        }
    }
}

/// A permit-to-target binding.
#[derive(Clone, Debug)]
pub struct PermitAssignment {
    pub permit_id: i32,
    pub guild_id: String,
    pub target_type: PermitTargetType,
    pub target_id: i64,
}

/// The shape of a permit node pattern.
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

/// Match a granted node pattern against a required node.
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Deny,
    Pass,
}

/// One row of the tier trace, mirroring lumi's `PermitTierExplanation`.
#[derive(Clone, Debug)]
pub struct TierExplanation {
    pub tier: String,
    pub verdict: Verdict,
    pub detail: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Decision {
    pub allowed: bool,
    pub rows: Vec<TierExplanation>,
}

#[derive(Clone, Debug)]
pub struct CheckContext<'a> {
    pub guild_id: String,
    pub user_id: u64,
    pub channel_id: Option<u64>,
    /// Role ids ordered high→low (Discord position desc).
    pub role_ids: &'a [u64],
    pub is_owner: bool,
    pub is_guild_owner: bool,
    pub is_manager: bool,
    pub is_quarantined: bool,
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
        "tempvc" => Some("tempvc.admin"),
        "lumi" | "repo" | "download" => Some("admin.*"),
        _ => None,
    }
}

/// The permit node a command requires.
///
/// A `&'static str` placed in the command's poise `custom_data` wins, so a
/// command (or subcommand) can declare its own node; otherwise the built-in
/// [`required_node`] name map is used.
#[must_use]
pub fn command_node(command: &crate::bot::Command) -> Option<&'static str> {
    command
        .custom_data
        .downcast_ref::<&'static str>()
        .copied()
        .or_else(|| required_node(&command.name))
}

#[derive(Debug)]
pub struct PermitResolver {
    permits: HashMap<i32, PermitRow>,
    by_user: HashMap<(String, i64), Vec<i32>>,
    by_channel: HashMap<(String, i64), Vec<i32>>,
    by_role: HashMap<(String, i64), Vec<i32>>,
}

fn tier_row(tier: &str, verdict: Verdict, detail: Option<String>) -> TierExplanation {
    TierExplanation {
        tier: tier.into(),
        verdict,
        detail,
    }
}

/// Bucket key: guild + numeric target id.
fn slot(guild_id: &str, id: u64) -> (String, i64) {
    (guild_id.to_string(), i64::try_from(id).unwrap_or(i64::MAX))
}

impl PermitResolver {
    pub async fn load(db: &DatabaseConnection) -> Result<Self> {
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
            .filter_map(|a| match a.target_id.parse::<i64>() {
                Ok(target_id) => Some(PermitAssignment {
                    permit_id: a.permit_id,
                    guild_id: a.guild_id,
                    target_type: a.target_type,
                    target_id,
                }),
                Err(_) => {
                    tracing::warn!(target = %a.target_id, permit_id = a.permit_id, "skipping permit assignment with non-numeric target");
                    None
                }
            })
            .collect();

        Ok(Self::new(permits, assignments))
    }

    pub fn new(permits: Vec<PermitRow>, assignments: Vec<PermitAssignment>) -> Self {
        let permit_map = permits.into_iter().map(|p| (p.id, p)).collect();

        let mut by_user: HashMap<(String, i64), Vec<i32>> = HashMap::new();
        let mut by_channel: HashMap<(String, i64), Vec<i32>> = HashMap::new();
        let mut by_role: HashMap<(String, i64), Vec<i32>> = HashMap::new();

        for a in assignments {
            let key = (a.guild_id, a.target_id);
            match a.target_type {
                PermitTargetType::User => by_user.entry(key).or_default().push(a.permit_id),
                PermitTargetType::Channel => by_channel.entry(key).or_default().push(a.permit_id),
                PermitTargetType::Role => by_role.entry(key).or_default().push(a.permit_id),
            }
        }

        Self {
            permits: permit_map,
            by_user,
            by_channel,
            by_role,
        }
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
        let mut rows = Vec::new();

        if ctx.is_owner {
            rows.push(tier_row("owner", Verdict::Allow, Some("bot owner".into())));
            return rows;
        }
        if ctx.is_guild_owner {
            rows.push(tier_row("owner", Verdict::Allow, Some("server owner".into())));
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
        let user = slot(&ctx.guild_id, ctx.user_id);

        consider(
            &mut rows,
            &mut decided,
            &mut allowed,
            self.evaluate_bucket("enforced:user", self.by_user.get(&user), ctx.node, true),
        );
        if decided || ctx.is_quarantined {
            if ctx.is_quarantined && !decided {
                rows.push(tier_row("quarantine", Verdict::Deny, Some("user is quarantined".into())));
            }
            return rows;
        }

        consider(
            &mut rows,
            &mut decided,
            &mut allowed,
            self.evaluate_bucket("custom:user", self.by_user.get(&user), ctx.node, false),
        );
        if decided {
            return rows;
        }

        if let Some(channel_id) = ctx.channel_id {
            let channel = slot(&ctx.guild_id, channel_id);
            consider(
                &mut rows,
                &mut decided,
                &mut allowed,
                self.evaluate_bucket("custom:channel", self.by_channel.get(&channel), ctx.node, false),
            );
            if decided {
                return rows;
            }
        }

        for &role_id in ctx.role_ids {
            let role = slot(&ctx.guild_id, role_id);
            consider(
                &mut rows,
                &mut decided,
                &mut allowed,
                self.evaluate_bucket(
                    &format!("custom:role:{role_id}"),
                    self.by_role.get(&role),
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
        permit_ids: Option<&Vec<i32>>,
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
            if let Some(pattern) = permit.nodes.iter().find(|pattern| node_matches(pattern, node)) {
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

        if let Some(pattern) = deny {
            tier_row(tier, Verdict::Deny, Some(format!("deny {pattern}")))
        } else if let Some(pattern) = grant {
            tier_row(tier, Verdict::Allow, Some(format!("grant {pattern}")))
        } else {
            tier_row(tier, Verdict::Pass, None)
        }
    }
}

/// The target of a permit explain walk, as much as the caller can cheaply gather.
#[derive(Clone, Debug)]
pub struct ExplainSubject {
    pub user_id: u64,
    pub channel_id: Option<u64>,
    pub role_ids: Vec<u64>,
    pub is_owner: bool,
    pub is_guild_owner: bool,
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
    let permits = lumi_db::list_permits(db, guild_id)
        .await?
        .into_iter()
        .map(PermitRow::from_model)
        .collect();

    let assignments = lumi_db::list_assignments(db, guild_id)
        .await?
        .into_iter()
        .filter_map(|row| match row.target_id.parse::<i64>() {
            Ok(target_id) => Some(PermitAssignment {
                permit_id: row.permit_id,
                guild_id: guild_id.to_string(),
                target_type: row.target_type,
                target_id,
            }),
            Err(_) => {
                tracing::warn!(target = %row.target_id, "skipping permit assignment with non-numeric target");
                None
            }
        })
        .collect();

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

    fn assignment(permit_id: i32, target_type: PermitTargetType, target_id: i64) -> PermitAssignment {
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
        let mut command = crate::bot::Command::default();
        command.name = "ban".into();
        assert_eq!(command_node(&command), Some("mod.ban"));

        command.custom_data = Box::new("custom.node");
        assert_eq!(command_node(&command), Some("custom.node"));
    }

    #[test]
    fn user_custom_grant_decides() {
        let permits = vec![permit(1, PermitKind::Custom, PermitPolarity::Grant, &["mod.ban"])];
        let resolver = PermitResolver::new(permits, vec![assignment(1, PermitTargetType::User, 7)]);
        let decision = resolver.resolve(&ctx(&[], "mod.ban"));
        assert!(decision.allowed);
        assert!(decision.rows.iter().any(|r| r.tier == "custom:user"));
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
        assert_eq!(decision.rows.last().map(|r| r.tier.as_str()), Some("enforced:user"));
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
        assert!(rows.iter().any(|r| r.tier == "manager" && r.verdict == Verdict::Pass));
    }
}
