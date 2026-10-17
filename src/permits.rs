//! Permit model and resolver, ported from lumi's `api/permissions/PermitResolver.ts`.
//!
//! Resolution order is fixed and mirrors the TS: owner bypass, then manager
//! (Administrator/ManageGuild) bypass, then `enforced` on the user target, then
//! `custom` buckets in chain order (user, channel, roles high→low). Deny wins
//! inside a bucket, and the first tier that returns a verdict decides; a tier
//! with no matching node passes to the next.

use std::collections::HashMap;

use sea_orm::entity::prelude::*;
use sea_orm::{DatabaseConnection, EntityTrait};

use crate::entities::{permit_assignments, permits};
use crate::Result;

/// String values match the Prisma `PermitKind` enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum PermitKind {
    #[sea_orm(string_value = "enforced")]
    Enforced,
    #[sea_orm(string_value = "custom")]
    Custom,
}

/// String values match the Prisma `PermitPolarity` enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum PermitPolarity {
    #[sea_orm(string_value = "grant")]
    Grant,
    #[sea_orm(string_value = "deny")]
    Deny,
}

/// String values match the Prisma `PermitTargetType` enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum PermitTargetType {
    #[sea_orm(string_value = "user")]
    User,
    #[sea_orm(string_value = "role")]
    Role,
    #[sea_orm(string_value = "channel")]
    Channel,
}

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
            node == namespace || node.strip_prefix(namespace).is_some_and(|rest| rest.starts_with('.'))
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

/// Everything needed to evaluate one permit node for one actor.
#[derive(Clone, Debug)]
pub struct PermitContext {
    pub is_owner: bool,
    pub is_manager: bool,
    pub user_id: i64,
    /// Role ids ordered high→low (Discord position desc).
    pub role_ids: Vec<i64>,
    pub channel_id: Option<i64>,
    /// Reserved: lumi's resolver does not consult the channel's category.
    pub category_id: Option<i64>,
    pub is_quarantined: bool,
    pub node: String,
}

#[derive(Clone, Debug, Default)]
struct PolarityBucket {
    grant: Vec<String>,
    deny: Vec<String>,
}

#[derive(Clone, Debug, Default)]
struct TierBuckets {
    custom: PolarityBucket,
    enforced: PolarityBucket,
}

/// Loaded permits and assignments, queried in memory.
///
/// Targets are globally unique Discord snowflakes, so a single resolver spans
/// every guild without keying by guild.
#[derive(Debug, Default)]
pub struct PermitResolver {
    permits: HashMap<i32, PermitRow>,
    assignments: Vec<PermitAssignment>,
}

impl PermitResolver {
    // shortcut: loads every guild's permits into memory; switch to per-guild
    // load + cache invalidation once a full permit map no longer fits.
    pub async fn load(db: &DatabaseConnection) -> Result<Self> {
        let permits = permits::Entity::find().all(db).await?;
        let assignments = permit_assignments::Entity::find().all(db).await?;

        let permits = permits
            .into_iter()
            .map(|m| {
                (
                    m.id,
                    PermitRow {
                        id: m.id,
                        name: m.name,
                        kind: m.kind,
                        polarity: m.polarity,
                        nodes: m.nodes,
                        builtin: m.builtin,
                        position: m.position,
                    },
                )
            })
            .collect();

        let assignments = assignments
            .into_iter()
            .map(|m| PermitAssignment {
                permit_id: m.permit_id,
                target_type: m.target_type,
                target_id: m.target_id,
            })
            .collect();

        Ok(Self {
            permits,
            assignments,
        })
    }

    pub fn resolve(&self, ctx: &PermitContext) -> Decision {
        if ctx.is_owner {
            return Decision {
                allowed: true,
                rows: vec![TierExplanation {
                    tier: "owner".to_string(),
                    verdict: Verdict::Allow,
                    detail: Some("bot owner".to_string()),
                }],
            };
        }

        let mut rows = vec![TierExplanation {
            tier: "owner".to_string(),
            verdict: Verdict::Pass,
            detail: None,
        }];

        if ctx.is_manager {
            rows.push(TierExplanation {
                tier: "manager".to_string(),
                verdict: Verdict::Allow,
                detail: Some("Administrator or ManageGuild".to_string()),
            });
            return Decision { allowed: true, rows };
        }
        rows.push(TierExplanation {
            tier: "manager".to_string(),
            verdict: Verdict::Pass,
            detail: None,
        });

        let mut targets: Vec<(PermitTargetType, i64)> =
            vec![(PermitTargetType::User, ctx.user_id)];
        let mut labels = vec!["user".to_string()];
        if let Some(channel_id) = ctx.channel_id {
            targets.push((PermitTargetType::Channel, channel_id));
            labels.push("channel".to_string());
        }
        for role_id in &ctx.role_ids {
            targets.push((PermitTargetType::Role, *role_id));
            labels.push(format!("role:{role_id}"));
        }

        let mut decided = false;
        let mut allowed = false;

        if let Some((target_type, target_id)) = targets.first() {
            let bucket = self.tier(*target_type, *target_id);
            let row = verdict_for("enforced:user", &bucket.enforced, &ctx.node);
            consider(&mut rows, &mut decided, &mut allowed, row);
        }

        for (index, (target_type, target_id)) in targets.iter().enumerate() {
            let bucket = self.tier(*target_type, *target_id);
            let row = verdict_for(&format!("custom:{}", labels[index]), &bucket.custom, &ctx.node);
            if ctx.is_quarantined {
                rows.push(quarantined(row));
                continue;
            }
            consider(&mut rows, &mut decided, &mut allowed, row);
        }

        Decision { allowed, rows }
    }

    fn tier(&self, target_type: PermitTargetType, target_id: i64) -> TierBuckets {
        let mut picked: Vec<&PermitRow> = self
            .assignments
            .iter()
            .filter(|a| a.target_type == target_type && a.target_id == target_id)
            .filter_map(|a| self.permits.get(&a.permit_id))
            .collect();
        picked.sort_by_key(|p| (p.position, p.id));

        let mut buckets = TierBuckets::default();
        for permit in picked {
            let kind_bucket = if permit.kind == PermitKind::Enforced {
                &mut buckets.enforced
            } else {
                &mut buckets.custom
            };
            let polarity_bucket = if permit.polarity == PermitPolarity::Deny {
                &mut kind_bucket.deny
            } else {
                &mut kind_bucket.grant
            };
            polarity_bucket.extend(permit.nodes.iter().cloned());
        }
        buckets
    }
}

fn verdict_for(tier: &str, bucket: &PolarityBucket, node: &str) -> TierExplanation {
    if let Some(matched) = bucket.deny.iter().find(|granted| node_matches(granted, node)) {
        return TierExplanation {
            tier: tier.to_string(),
            verdict: Verdict::Deny,
            detail: Some(format!("deny {matched}")),
        };
    }
    if let Some(matched) = bucket.grant.iter().find(|granted| node_matches(granted, node)) {
        return TierExplanation {
            tier: tier.to_string(),
            verdict: Verdict::Allow,
            detail: Some(format!("grant {matched}")),
        };
    }
    TierExplanation {
        tier: tier.to_string(),
        verdict: Verdict::Pass,
        detail: None,
    }
}

fn consider(
    rows: &mut Vec<TierExplanation>,
    decided: &mut bool,
    allowed: &mut bool,
    row: TierExplanation,
) {
    if !*decided && row.verdict != Verdict::Pass {
        *decided = true;
        *allowed = row.verdict == Verdict::Allow;
    }
    rows.push(row);
}

fn quarantined(mut row: TierExplanation) -> TierExplanation {
    row.detail = Some(row.detail.take().map_or_else(
        || "ignored while quarantined".to_string(),
        |detail| format!("{detail}, ignored while quarantined"),
    ));
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: i32, kind: PermitKind, polarity: PermitPolarity, nodes: &[&str], position: i32) -> PermitRow {
        PermitRow {
            id,
            name: format!("p{id}"),
            kind,
            polarity,
            nodes: nodes.iter().map(|s| (*s).to_string()).collect(),
            builtin: false,
            position,
        }
    }

    fn resolver(rows: Vec<PermitRow>, assignments: Vec<PermitAssignment>) -> PermitResolver {
        PermitResolver {
            permits: rows.into_iter().map(|r| (r.id, r)).collect(),
            assignments,
        }
    }

    #[test]
    fn node_matches_wildcard_exact_namespace() {
        assert!(node_matches("*", "mod.ban"));
        assert!(node_matches("mod.ban", "mod.ban"));
        assert!(!node_matches("mod.ban", "mod.kick"));
        assert!(node_matches("admin.*", "admin.settings.edit"));
        assert!(node_matches("admin.*", "admin"));
        assert!(!node_matches("admin.*", "adminx"));
        assert!(!node_matches("", "mod.ban"));
        assert!(!node_matches("mod.ban", ""));
    }

    #[test]
    fn owner_and_manager_bypass() {
        let r = PermitResolver::default();
        let owner = r.resolve(&PermitContext {
            is_owner: true,
            is_manager: false,
            user_id: 1,
            role_ids: vec![],
            channel_id: None,
            category_id: None,
            is_quarantined: false,
            node: "mod.ban".to_string(),
        });
        assert!(owner.allowed);
        assert_eq!(owner.rows[0].verdict, Verdict::Allow);

        let manager = r.resolve(&PermitContext {
            is_owner: false,
            is_manager: true,
            user_id: 2,
            role_ids: vec![],
            channel_id: None,
            category_id: None,
            is_quarantined: false,
            node: "mod.ban".to_string(),
        });
        assert!(manager.allowed);
        assert_eq!(manager.rows[1].verdict, Verdict::Allow);
    }

    fn ctx(user_id: i64, role_ids: Vec<i64>, node: &str) -> PermitContext {
        PermitContext {
            is_owner: false,
            is_manager: false,
            user_id,
            role_ids,
            channel_id: None,
            category_id: None,
            is_quarantined: false,
            node: node.to_string(),
        }
    }

    #[test]
    fn deny_wins_within_bucket_and_first_tier_decides() {
        let rows = vec![
            row(1, PermitKind::Custom, PermitPolarity::Deny, &["mod.*"], 0),
            row(2, PermitKind::Custom, PermitPolarity::Grant, &["mod.ban"], 1),
        ];
        let r = resolver(
            rows,
            vec![
                PermitAssignment {
                    permit_id: 1,
                    target_type: PermitTargetType::User,
                    target_id: 42,
                },
                PermitAssignment {
                    permit_id: 2,
                    target_type: PermitTargetType::Role,
                    target_id: 7,
                },
            ],
        );
        assert!(
            !r.resolve(&ctx(42, vec![7], "mod.ban")).allowed,
            "user-tier deny decides before the role-tier grant"
        );
        assert!(
            !r.resolve(&ctx(42, vec![7], "mod.kick")).allowed,
            "user-tier deny matches the namespace"
        );
        assert!(
            r.resolve(&ctx(99, vec![7], "mod.ban")).allowed,
            "no user assignment: role-tier grant decides"
        );

        let quarantine = r.resolve(&PermitContext {
            is_quarantined: true,
            ..ctx(99, vec![7], "mod.ban")
        });
        assert!(!quarantine.allowed, "custom tiers are ignored while quarantined");
        assert!(quarantine
            .rows
            .iter()
            .any(|row| row.detail.as_deref() == Some("grant mod.ban, ignored while quarantined")));
    }

    #[test]
    fn enforced_user_tier_is_checked_before_custom() {
        let rows = vec![
            row(1, PermitKind::Enforced, PermitPolarity::Deny, &["*"], 0),
            row(2, PermitKind::Custom, PermitPolarity::Grant, &["*"], 0),
        ];
        let r = resolver(
            rows,
            vec![
                PermitAssignment {
                    permit_id: 1,
                    target_type: PermitTargetType::User,
                    target_id: 5,
                },
                PermitAssignment {
                    permit_id: 2,
                    target_type: PermitTargetType::User,
                    target_id: 5,
                },
            ],
        );
        assert!(!r.resolve(&ctx(5, vec![], "mod.ban")).allowed);
    }
}
