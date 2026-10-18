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
    pub name: String,
    pub kind: PermitKind,
    pub polarity: PermitPolarity,
    pub nodes: Vec<String>,
    pub builtin: bool,
    pub position: i32,
}

/// A permit-to-target binding.
#[derive(Clone, Copy, Debug)]
pub struct PermitAssignment {
    pub permit_id: i32,
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
    pub user_id: u64,
    pub channel_id: Option<u64>,
    /// Role ids ordered high→low (Discord position desc).
    pub role_ids: &'a [u64],
    pub is_owner: bool,
    pub is_manager: bool,
    pub is_quarantined: bool,
    pub node: &'a str,
}

#[derive(Debug)]
pub struct PermitResolver {
    permits: HashMap<i32, PermitRow>,
    by_user: HashMap<i64, Vec<i32>>,
    by_channel: HashMap<i64, Vec<i32>>,
    by_role: HashMap<i64, Vec<i32>>,
}

impl PermitResolver {
    pub async fn load(db: &DatabaseConnection) -> Result<Self> {
        let permit_models = permit::Entity::find().all(db).await?;
        let assign_models = permit_assignment::Entity::find().all(db).await?;

        let permit_rows = permit_models
            .into_iter()
            .map(|p| PermitRow {
                id: p.id,
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

        let mut by_user: HashMap<i64, Vec<i32>> = HashMap::new();
        let mut by_channel: HashMap<i64, Vec<i32>> = HashMap::new();
        let mut by_role: HashMap<i64, Vec<i32>> = HashMap::new();

        for a in assignments {
            match a.target_type {
                PermitTargetType::User => by_user.entry(a.target_id).or_default().push(a.permit_id),
                PermitTargetType::Channel => {
                    by_channel.entry(a.target_id).or_default().push(a.permit_id);
                }
                PermitTargetType::Role => by_role.entry(a.target_id).or_default().push(a.permit_id),
            }
        }

        Self {
            permits: permit_map,
            by_user,
            by_channel,
            by_role,
        }
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
        let enforced_row = self.evaluate_bucket("enforced", self.by_user.get(&(ctx.user_id as i64)), ctx.node, true);
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
        let user_row = self.evaluate_bucket("user", self.by_user.get(&(ctx.user_id as i64)), ctx.node, false);
        consider(&mut rows, &mut decided, &mut allowed, user_row);
        if decided {
            return Decision { allowed, rows };
        }

        // Tier: Channel Custom
        if let Some(channel_id) = ctx.channel_id {
            let channel_row = self.evaluate_bucket("channel", self.by_channel.get(&(channel_id as i64)), ctx.node, false);
            consider(&mut rows, &mut decided, &mut allowed, channel_row);
            if decided {
                return Decision { allowed, rows };
            }
        }

        // Tier: Roles Custom (in priority order)
        for &role_id in ctx.role_ids {
            let role_row = self.evaluate_bucket(&format!("role:{role_id}"), self.by_role.get(&(role_id as i64)), ctx.node, false);
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
