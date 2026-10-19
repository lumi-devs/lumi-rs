//! Permit model and resolver, ported from lumi's `api/permissions/PermitResolver.ts`.
//!
//! Resolution order is fixed and mirrors the TS: owner bypass, then manager
//! (Administrator/ManageGuild) bypass, then `enforced` on the user target, then
//! `custom` buckets in chain order (user, channel, roles high→low). Deny wins
//! inside a bucket, and the first tier that returns a verdict decides; a tier
//! with no matching node passes to the next.

use std::collections::HashMap;

use sea_orm::{DatabaseConnection, EntityTrait};

pub use lumi_db::entities::enums::{PermitKind, PermitPolarity, PermitTargetType};
use lumi_db::entities::{permit_assignment, permit};
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
        NodeScope::Namespace => {
            let namespace = &pattern[..pattern.len() - 2];
            node == namespace
                || node
                    .strip_prefix(namespace)
                    .is_some_and(|rest| rest.starts_with('.'))
        }
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

#[derive(Debug)]
pub struct PermitResolver {
    permits: HashMap<i32, PermitRow>,
    by_user: HashMap<(String, i64), Vec<i32>>,
    by_channel: HashMap<(String, i64), Vec<i32>>,
    by_role: HashMap<(String, i64), Vec<i32>>,
}

impl PermitResolver {
    pub async fn load(db: &DatabaseConnection) -> Result<Self> {
        let permit_models = permit::Entity::find().all(db).await?;
        let assign_models = permit_assignment::Entity::find().all(db).await?;

        let permit_rows = permit_models
            .into_iter()
            .map(|p| PermitRow {
                id: p.id,
                guild_id: p.guild_id,
                name: p.name,
                kind: p.kind,
                polarity: p.polarity,
                nodes: p.nodes,
                builtin: p.builtin,
                position: p.position,
            })
            .collect();

        let assignments = assign_models
            .into_iter()
            .map(|a| PermitAssignment {
                permit_id: a.permit_id,
                guild_id: a.guild_id,
                target_type: a.target_type,
                target_id: a.target_id.parse().unwrap_or(0),
            })
            .collect();

        Ok(Self::new(permit_rows, assignments))
    }

    pub fn new(permits: Vec<PermitRow>, assignments: Vec<PermitAssignment>) -> Self {
        let mut permit_map = HashMap::new();
        for p in permits {
            permit_map.insert(p.id, p);
        }

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

    /// Every permit belonging to one guild (the TS `getPermitChain` read side).
    pub fn permits_for_guild(&self, guild_id: &str) -> Vec<&PermitRow> {
        self.permits
            .values()
            .filter(|p| p.guild_id == guild_id)
            .collect()
    }

    pub fn resolve(&self, ctx: &CheckContext<'_>) -> Decision {
        let mut rows = Vec::new();

        if ctx.is_owner {
            rows.push(TierExplanation {
                tier: "owner".into(),
                verdict: Verdict::Allow,
                detail: Some("bot owner bypass".into()),
            });
            return Decision {
                allowed: true,
                rows,
            };
        }

        if ctx.is_guild_owner {
            rows.push(TierExplanation {
                tier: "owner".into(),
                verdict: Verdict::Allow,
                detail: Some("server owner bypass".into()),
            });
            return Decision {
                allowed: true,
                rows,
            };
        }

        if ctx.is_manager {
            rows.push(TierExplanation {
                tier: "manager".into(),
                verdict: Verdict::Allow,
                detail: Some("server manager bypass".into()),
            });
            return Decision {
                allowed: true,
                rows,
            };
        }

        let mut decided = false;
        let mut allowed = false;

        // Tier: Enforced (User target only)
        let enforced_row = self.evaluate_bucket(
            "enforced",
            self.by_user.get(&(ctx.guild_id.clone(), ctx.user_id as i64)),
            ctx.node,
            true,
        );
        consider(&mut rows, &mut decided, &mut allowed, enforced_row);

        if decided || ctx.is_quarantined {
            if ctx.is_quarantined && !decided {
                rows.push(TierExplanation {
                    tier: "quarantine".into(),
                    verdict: Verdict::Deny,
                    detail: Some("user is quarantined".into()),
                });
            }
            return Decision { allowed, rows };
        }

        // Tier: User Custom
        let user_row = self.evaluate_bucket(
            "user",
            self.by_user.get(&(ctx.guild_id.clone(), ctx.user_id as i64)),
            ctx.node,
            false,
        );
        consider(&mut rows, &mut decided, &mut allowed, user_row);
        if decided {
            return Decision { allowed, rows };
        }

        // Tier: Channel Custom
        if let Some(channel_id) = ctx.channel_id {
            let channel_row = self.evaluate_bucket(
                "channel",
                self.by_channel.get(&(ctx.guild_id.clone(), channel_id as i64)),
                ctx.node,
                false,
            );
            consider(&mut rows, &mut decided, &mut allowed, channel_row);
            if decided {
                return Decision { allowed, rows };
            }
        }

        // Tier: Roles Custom (in priority order)
        for &role_id in ctx.role_ids {
            let role_row = self.evaluate_bucket(
                &format!("role:{role_id}"),
                self.by_role.get(&(ctx.guild_id.clone(), role_id as i64)),
                ctx.node,
                false,
            );
            consider(&mut rows, &mut decided, &mut allowed, role_row);
            if decided {
                return Decision { allowed, rows };
            }
        }

        Decision { allowed, rows }
    }

    fn evaluate_bucket(&self, tier: &str, permit_ids: Option<&Vec<i32>>, node: &str, enforced_only: bool) -> TierExplanation {
        let Some(ids) = permit_ids else {
            return TierExplanation {
                tier: tier.into(),
                verdict: Verdict::Pass,
                detail: None,
            };
        };

        let mut has_grant = false;
        let mut has_deny = false;

        for &id in ids {
            if let Some(permit) = self.permits.get(&id) {
                if enforced_only && permit.kind != PermitKind::Enforced {
                    continue;
                }
                if !enforced_only && permit.kind != PermitKind::Custom {
                    continue;
                }

                if permit.nodes.iter().any(|pattern| node_matches(pattern, node)) {
                    match permit.polarity {
                        PermitPolarity::Deny => has_deny = true,
                        PermitPolarity::Grant => has_grant = true,
                    }
                }
            }
        }

        if has_deny {
            TierExplanation {
                tier: tier.into(),
                verdict: Verdict::Deny,
                detail: Some("denied by node match".into()),
            }
        } else if has_grant {
            TierExplanation {
                tier: tier.into(),
                verdict: Verdict::Allow,
                detail: Some("granted by node match".into()),
            }
        } else {
            TierExplanation {
                tier: tier.into(),
                verdict: Verdict::Pass,
                detail: None,
            }
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
    let permits: HashMap<i32, permit::Model> = lumi_db::list_permits(db, guild_id)
        .await?
        .into_iter()
        .map(|p| (p.id, p))
        .collect();
    let assignments = lumi_db::list_assignments(db, guild_id).await?;

    let mut rows = Vec::new();
    if subject.is_owner {
        rows.push(TierExplanation { tier: "owner".into(), verdict: Verdict::Allow, detail: Some("bot owner".into()) });
        return Ok(rows);
    }
    if subject.is_guild_owner {
        rows.push(TierExplanation { tier: "owner".into(), verdict: Verdict::Allow, detail: Some("server owner".into()) });
        return Ok(rows);
    }
    rows.push(tier_row("owner", Verdict::Pass, None));
    if subject.is_manager {
        rows.push(TierExplanation {
            tier: "manager".into(),
            verdict: Verdict::Allow,
            detail: Some("Administrator or ManageGuild".into()),
        });
        return Ok(rows);
    }
    rows.push(tier_row("manager", Verdict::Pass, None));

    let mut decided = false;
    let user = subject.user_id.to_string();

    push_tier(
        &mut rows,
        &mut decided,
        "enforced:user",
        tier_verdict(&permits, &assignments, PermitKind::Enforced, node, |a| {
            a.target_type == PermitTargetType::User && a.target_id == user
        }),
    );
    if decided {
        return Ok(rows);
    }

    push_tier(
        &mut rows,
        &mut decided,
        "custom:user",
        tier_verdict(&permits, &assignments, PermitKind::Custom, node, |a| {
            a.target_type == PermitTargetType::User && a.target_id == user
        }),
    );
    if decided {
        return Ok(rows);
    }

    if let Some(channel_id) = subject.channel_id {
        let channel = channel_id.to_string();
        push_tier(
            &mut rows,
            &mut decided,
            "custom:channel",
            tier_verdict(&permits, &assignments, PermitKind::Custom, node, |a| {
                a.target_type == PermitTargetType::Channel && a.target_id == channel
            }),
        );
        if decided {
            return Ok(rows);
        }
    }

    for role_id in &subject.role_ids {
        let role = role_id.to_string();
        push_tier(
            &mut rows,
            &mut decided,
            &format!("custom:role:{role_id}"),
            tier_verdict(&permits, &assignments, PermitKind::Custom, node, |a| {
                a.target_type == PermitTargetType::Role && a.target_id == role
            }),
        );
        if decided {
            return Ok(rows);
        }
    }

    Ok(rows)
}

/// Deny wins inside one bucket, then grant, else the tier passes.
fn tier_verdict(
    permits: &HashMap<i32, permit::Model>,
    assignments: &[lumi_db::AssignmentRow],
    kind: PermitKind,
    node: &str,
    target: impl Fn(&lumi_db::AssignmentRow) -> bool,
) -> (Verdict, Option<String>) {
    let mut grant = None;
    let mut deny = None;
    for assignment in assignments.iter().filter(|a| a.kind == kind && target(a)) {
        let Some(permit) = permits.get(&assignment.permit_id) else {
            continue;
        };
        if let Some(pattern) = permit.nodes.iter().find(|p| node_matches(p, node)) {
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
        (Verdict::Deny, Some(format!("deny {pattern}")))
    } else if let Some(pattern) = grant {
        (Verdict::Allow, Some(format!("grant {pattern}")))
    } else {
        (Verdict::Pass, None)
    }
}

fn tier_row(tier: &str, verdict: Verdict, detail: Option<String>) -> TierExplanation {
    TierExplanation { tier: tier.into(), verdict, detail }
}

fn push_tier(rows: &mut Vec<TierExplanation>, decided: &mut bool, tier: &str, verdict: (Verdict, Option<String>)) {
    if *decided {
        return;
    }
    let (verdict, detail) = verdict;
    if verdict != Verdict::Pass {
        *decided = true;
    }
    rows.push(tier_row(tier, verdict, detail));
}

fn consider(rows: &mut Vec<TierExplanation>, decided: &mut bool, allowed: &mut bool, row: TierExplanation) {
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
