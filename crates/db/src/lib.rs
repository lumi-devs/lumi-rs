pub mod entities;
pub mod ids;
pub mod migrations;

use std::collections::HashMap;
use std::time::Duration;

use sea_orm::entity::prelude::{ChronoUtc, DateTimeWithTimeZone};
use sea_orm::sea_query::{Expr, OnConflict};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectOptions, ConnectionTrait, Database,
    DatabaseBackend as DbBackend, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QueryResult,
    Set, Statement, TransactionTrait,
};
use sea_orm_migration::MigratorTrait;

pub use sea_orm::DatabaseConnection;

use crate::entities::enums::{PermitKind, PermitTargetType};
use crate::entities::{permit, permit_assignment};
use crate::ids::{GuildId, SnowflakeId};
use crate::migrations::Migrator;

pub type Result<T> = std::result::Result<T, sea_orm::DbErr>;

/// Read a numeric env var, warning and falling back to `default` when it does
/// not parse.
fn env_number<T: std::str::FromStr>(name: &str, default: T) -> T {
    match std::env::var(name) {
        Ok(raw) => raw.trim().parse().unwrap_or_else(|_| {
            eprintln!("warning: invalid {name}={raw:?}; using the default");
            default
        }),
        Err(_) => default,
    }
}

fn env_text(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_owned())
}

fn options(url: &str) -> ConnectOptions {
    let mut options = ConnectOptions::new(url.to_owned());
    options
        .max_connections(env_number("DATABASE_MAX_CONNECTIONS", 12))
        .min_connections(env_number("DATABASE_MIN_CONNECTIONS", 2))
        .connect_timeout(Duration::from_millis(env_number(
            "DATABASE_CONNECT_TIMEOUT_MS",
            5_000,
        )))
        .acquire_timeout(Duration::from_millis(env_number(
            "DATABASE_ACQUIRE_TIMEOUT_MS",
            3_000,
        )))
        .idle_timeout(Duration::from_secs(env_number(
            "DATABASE_IDLE_TIMEOUT_SECS",
            300,
        )))
        .max_lifetime(Duration::from_secs(env_number(
            "DATABASE_MAX_LIFETIME_SECS",
            1_800,
        )))
        .sqlx_logging(false)
        .set_application_name(env_text("DATABASE_APP_NAME", "lumi"));
    options
}

pub async fn connect(url: &str) -> Result<DatabaseConnection> {
    Database::connect(options(url)).await
}

/// Connect a read replica with the same pool options as [`connect`].
pub async fn connect_read(url: &str) -> Result<DatabaseConnection> {
    Database::connect(options(url)).await
}

pub async fn migrate(db: &DatabaseConnection) -> Result<()> {
    Migrator::up(db, None).await?;
    Ok(())
}

/// Current UTC timestamp in the form every `DateTimeWithTimeZone` column stores.
pub fn now() -> DateTimeWithTimeZone {
    ChronoUtc::now().fixed_offset()
}

/// Atomically increments and returns the next value of a named per-guild counter.
///
/// A single upsert replaces the old `FOR UPDATE` transaction: Postgres either
/// updates and returns the row, or returns nothing when the value would leave
/// the `i64` range.
pub async fn next_counter(db: &DatabaseConnection, guild_id: u64, name: &str) -> Result<i64> {
    let guild_id = GuildId::from(guild_id);
    let statement = Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO counters (guild_id, name, value) VALUES ($1, $2, 1) \
         ON CONFLICT (guild_id, name) DO UPDATE SET value = counters.value + 1 \
         WHERE counters.value < $3 \
         RETURNING value",
        [guild_id.into(), name.into(), i64::MAX.into()],
    );
    let Some(row) = db.query_one_raw(statement).await? else {
        return Err(sea_orm::DbErr::Custom(format!(
            "counter `{name}` for guild {guild_id} overflowed i64"
        )));
    };
    row.try_get::<i64>("", "value")
}

/// Look up the prefix for a guild, falling back to `global_config.default_prefix`, then ",".
pub async fn get_prefix(db: &DatabaseConnection, guild_id: Option<u64>) -> Result<String> {
    if let Some(gid) = guild_id {
        let prefix = entities::guild::Entity::find_by_id(GuildId::from(gid))
            .one(db)
            .await?
            .and_then(|guild| guild.prefix)
            .filter(|prefix| !prefix.is_empty());
        if let Some(prefix) = prefix {
            return Ok(prefix);
        }
    }

    if let Some(global) = entities::global::Entity::find()
        .one(db)
        .await?
        .filter(|g| !g.default_prefix.is_empty())
    {
        return Ok(global.default_prefix);
    }

    Ok(",".to_owned())
}

/// Set a guild's command prefix, creating the guild row when it does not exist.
pub async fn set_prefix(db: &DatabaseConnection, guild_id: u64, prefix: &str) -> Result<()> {
    let timestamp = now();
    entities::guild::Entity::insert(entities::guild::ActiveModel {
        id: Set(GuildId::from(guild_id)),
        prefix: Set(Some(prefix.to_owned())),
        locale: Set("en-US".to_owned()),
        ignored: Set(false),
        left_at: Set(None),
        created_at: Set(timestamp),
        updated_at: Set(timestamp),
    })
    .on_conflict(
        OnConflict::column(entities::guild::Column::Id)
            .update_columns([
                entities::guild::Column::Prefix,
                entities::guild::Column::UpdatedAt,
            ])
            .to_owned(),
    )
    .exec(db)
    .await?;
    Ok(())
}

/// Clear a guild's prefix override so it falls back to the global default.
pub async fn reset_prefix(db: &DatabaseConnection, guild_id: u64) -> Result<()> {
    entities::guild::Entity::update_many()
        .col_expr(
            entities::guild::Column::Prefix,
            Expr::value(Option::<String>::None),
        )
        .col_expr(entities::guild::Column::UpdatedAt, Expr::value(now()))
        .filter(entities::guild::Column::Id.eq(GuildId::from(guild_id)))
        .exec(db)
        .await?;
    Ok(())
}

pub async fn count_enabled_modules(db: &DatabaseConnection, guild_id: u64) -> Result<u64> {
    entities::guild_module_state::Entity::find()
        .filter(entities::guild_module_state::Column::GuildId.eq(GuildId::from(guild_id)))
        .filter(entities::guild_module_state::Column::Enabled.eq(true))
        .count(db)
        .await
}

/// Server version reported by the connected PostgreSQL, e.g. `18.6`.
pub async fn server_version(db: &DatabaseConnection) -> Result<String> {
    let row = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SHOW server_version".to_owned(),
        ))
        .await?;
    Ok(row
        .and_then(|row| row.try_get::<String>("", "server_version").ok())
        .unwrap_or_else(|| "unknown".to_string()))
}

/// Lifetime commit total of the connected database, for the dashboard's
/// transaction rate.
pub async fn committed_transactions(db: &DatabaseConnection) -> Result<u64> {
    let row = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT xact_commit AS commits FROM pg_stat_database WHERE datname = current_database()"
                .to_owned(),
        ))
        .await?;
    Ok(row.as_ref().map_or(0, |row| counter(row, "commits")))
}

/// Read a non-negative `bigint` column, treating anything unexpected as zero.
fn counter(row: &QueryResult, column: &str) -> u64 {
    row.try_get::<i64>("", column)
        .ok()
        .and_then(|value| u64::try_from(value).ok())
        .unwrap_or(0)
}

pub async fn list_permits(
    db: &DatabaseConnection,
    guild_id: u64,
) -> Result<Vec<entities::permit::Model>> {
    entities::permit::Entity::find()
        .filter(entities::permit::Column::GuildId.eq(GuildId::from(guild_id)))
        .order_by_asc(entities::permit::Column::Position)
        .all(db)
        .await
}

pub async fn delete_permit(db: &DatabaseConnection, id: i32) -> Result<u64> {
    let result = entities::permit::Entity::delete_by_id(id).exec(db).await?;
    Ok(result.rows_affected)
}

/// A permit assignment joined with its permit's display fields.
#[derive(Clone, Debug)]
pub struct AssignmentRow {
    pub permit_id: i32,
    pub permit_name: String,
    pub kind: PermitKind,
    pub builtin: bool,
    pub target_type: PermitTargetType,
    pub target_id: SnowflakeId,
}

/// Every assignment in a guild, grouped under its permit in permit position
/// order. Two queries, grouped in memory, instead of a query per permit.
pub async fn list_assignments(
    db: &DatabaseConnection,
    guild_id: u64,
) -> Result<Vec<AssignmentRow>> {
    let permits = list_permits(db, guild_id).await?;
    let assignments = permit_assignment::Entity::find()
        .filter(permit_assignment::Column::GuildId.eq(GuildId::from(guild_id)))
        .all(db)
        .await?;

    let mut by_permit: HashMap<i32, Vec<&permit_assignment::Model>> = HashMap::new();
    for assignment in &assignments {
        by_permit
            .entry(assignment.permit_id)
            .or_default()
            .push(assignment);
    }

    let mut rows = Vec::with_capacity(assignments.len());
    for permit in &permits {
        for assignment in by_permit.get(&permit.id).into_iter().flatten() {
            rows.push(AssignmentRow {
                permit_id: permit.id,
                permit_name: permit.name.clone(),
                kind: permit.kind,
                builtin: permit.builtin,
                target_type: assignment.target_type,
                target_id: assignment.target_id,
            });
        }
    }
    Ok(rows)
}

pub async fn get_permit(
    db: &DatabaseConnection,
    guild_id: u64,
    permit_id: i32,
) -> Result<Option<permit::Model>> {
    permit::Entity::find_by_id(permit_id)
        .filter(permit::Column::GuildId.eq(GuildId::from(guild_id)))
        .one(db)
        .await
}

/// Bind a permit to a target, doing nothing when the binding already exists.
pub async fn assign_permit(
    db: &DatabaseConnection,
    guild_id: u64,
    permit_id: i32,
    target_type: PermitTargetType,
    target_id: SnowflakeId,
) -> Result<()> {
    let now = now();
    permit_assignment::Entity::insert(permit_assignment::ActiveModel {
        permit_id: Set(permit_id),
        guild_id: Set(GuildId::from(guild_id)),
        target_type: Set(target_type),
        target_id: Set(target_id),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    })
    .on_conflict(
        OnConflict::columns([
            permit_assignment::Column::PermitId,
            permit_assignment::Column::TargetType,
            permit_assignment::Column::TargetId,
        ])
        .do_nothing()
        .to_owned(),
    )
    .exec(db)
    .await?;
    Ok(())
}

pub async fn unassign_permit(
    db: &DatabaseConnection,
    guild_id: u64,
    permit_id: i32,
    target_type: PermitTargetType,
    target_id: SnowflakeId,
) -> Result<()> {
    permit_assignment::Entity::delete_many()
        .filter(permit_assignment::Column::GuildId.eq(GuildId::from(guild_id)))
        .filter(permit_assignment::Column::PermitId.eq(permit_id))
        .filter(permit_assignment::Column::TargetType.eq(target_type))
        .filter(permit_assignment::Column::TargetId.eq(target_id))
        .exec(db)
        .await?;
    Ok(())
}

pub async fn list_permits_for_kind(
    db: &DatabaseConnection,
    guild_id: u64,
    kind: PermitKind,
) -> Result<Vec<permit::Model>> {
    permit::Entity::find()
        .filter(permit::Column::GuildId.eq(GuildId::from(guild_id)))
        .filter(permit::Column::Kind.eq(kind))
        .order_by_asc(permit::Column::Position)
        .all(db)
        .await
}

/// Move a permit one step up/down within its guild by re-normalising `position`.
pub async fn move_permit(
    db: &DatabaseConnection,
    guild_id: u64,
    permit_id: i32,
    direction: &str,
) -> Result<()> {
    let txn = db.begin().await?;
    let mut permits = permit::Entity::find()
        .filter(permit::Column::GuildId.eq(GuildId::from(guild_id)))
        .order_by_asc(permit::Column::Position)
        .all(&txn)
        .await?;

    let Some(index) = permits.iter().position(|permit| permit.id == permit_id) else {
        return Ok(());
    };
    let target = match direction {
        "up" if index > 0 => index - 1,
        "down" if index + 1 < permits.len() => index + 1,
        _ => return Ok(()),
    };
    permits.swap(index, target);

    let timestamp = now();
    for (position, permit) in permits.iter().enumerate() {
        let position = i32::try_from(position)
            .map_err(|_| sea_orm::DbErr::Custom("permit position overflowed i32".to_owned()))?;
        if permit.position != position {
            let mut active: permit::ActiveModel = permit.clone().into();
            active.position = Set(position);
            active.updated_at = Set(timestamp);
            active.update(&txn).await?;
        }
    }
    txn.commit().await?;
    Ok(())
}
