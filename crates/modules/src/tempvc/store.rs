use chrono::Utc;
use sea_orm::{
    sea_query::OnConflict, ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait,
    QueryFilter, QueryOrder, Set,
};

use lumi_db::entities::{temp_vc_generator, temp_vc_record};
use lumi_kernel::Result;

pub async fn create_generator(
    db: &DatabaseConnection,
    guild_id: u64,
    channel_id: u64,
    name: String,
    limit: i32,
) -> Result<()> {
    let row = temp_vc_generator::ActiveModel {
        guild_id: Set(guild_id.to_string()),
        channel_id: Set(channel_id.to_string()),
        name: Set(name),
        limit: Set(limit),
    };

    temp_vc_generator::Entity::insert(row)
        .on_conflict(
            OnConflict::columns([
                temp_vc_generator::Column::GuildId,
                temp_vc_generator::Column::ChannelId,
            ])
            .update_columns([
                temp_vc_generator::Column::Name,
                temp_vc_generator::Column::Limit,
            ])
            .to_owned(),
        )
        .exec(db)
        .await?;
    Ok(())
}

pub async fn get_generator(
    db: &DatabaseConnection,
    guild_id: u64,
    channel_id: u64,
) -> Result<Option<temp_vc_generator::Model>> {
    Ok(temp_vc_generator::Entity::find()
        .filter(temp_vc_generator::Column::GuildId.eq(guild_id.to_string()))
        .filter(temp_vc_generator::Column::ChannelId.eq(channel_id.to_string()))
        .one(db)
        .await?)
}

pub async fn list_generators(
    db: &DatabaseConnection,
    guild_id: u64,
) -> Result<Vec<temp_vc_generator::Model>> {
    Ok(temp_vc_generator::Entity::find()
        .filter(temp_vc_generator::Column::GuildId.eq(guild_id.to_string()))
        .order_by_asc(temp_vc_generator::Column::Name)
        .all(db)
        .await?)
}

pub async fn update_generator(
    db: &DatabaseConnection,
    guild_id: u64,
    channel_id: u64,
    name: Option<String>,
    limit: Option<i32>,
) -> Result<bool> {
    let gen = get_generator(db, guild_id, channel_id).await?;
    if let Some(m) = gen {
        let mut active: temp_vc_generator::ActiveModel = m.into();
        if let Some(n) = name {
            active.name = Set(n);
        }
        if let Some(l) = limit {
            active.limit = Set(l);
        }
        active.update(db).await?;
        Ok(true)
    } else {
        Ok(false)
    }
}

pub async fn delete_generator(
    db: &DatabaseConnection,
    guild_id: u64,
    channel_id: u64,
) -> Result<bool> {
    let result = temp_vc_generator::Entity::delete_many()
        .filter(temp_vc_generator::Column::GuildId.eq(guild_id.to_string()))
        .filter(temp_vc_generator::Column::ChannelId.eq(channel_id.to_string()))
        .exec(db)
        .await?;
    Ok(result.rows_affected > 0)
}

pub async fn create_record(
    db: &DatabaseConnection,
    guild_id: u64,
    channel_id: u64,
    owner_id: u64,
    generator_id: u64,
    name: String,
    number: i32,
    locked: bool,
    hidden: bool,
) -> Result<()> {
    let row = temp_vc_record::ActiveModel {
        guild_id: Set(guild_id.to_string()),
        channel_id: Set(channel_id.to_string()),
        owner_id: Set(owner_id.to_string()),
        generator_id: Set(generator_id.to_string()),
        name: Set(name),
        number: Set(number),
        locked: Set(locked),
        hidden: Set(hidden),
        created_at: Set(Utc::now().fixed_offset()),
    };

    temp_vc_record::Entity::insert(row)
        .on_conflict(
            OnConflict::columns([
                temp_vc_record::Column::GuildId,
                temp_vc_record::Column::ChannelId,
            ])
            .update_columns([
                temp_vc_record::Column::OwnerId,
                temp_vc_record::Column::Name,
                temp_vc_record::Column::Number,
                temp_vc_record::Column::Locked,
                temp_vc_record::Column::Hidden,
                temp_vc_record::Column::CreatedAt,
            ])
            .to_owned(),
        )
        .exec(db)
        .await?;
    Ok(())
}

pub async fn get_record(
    db: &DatabaseConnection,
    guild_id: u64,
    channel_id: u64,
) -> Result<Option<temp_vc_record::Model>> {
    Ok(temp_vc_record::Entity::find()
        .filter(temp_vc_record::Column::GuildId.eq(guild_id.to_string()))
        .filter(temp_vc_record::Column::ChannelId.eq(channel_id.to_string()))
        .one(db)
        .await?)
}

pub async fn list_records(
    db: &DatabaseConnection,
    guild_id: u64,
) -> Result<Vec<temp_vc_record::Model>> {
    Ok(temp_vc_record::Entity::find()
        .filter(temp_vc_record::Column::GuildId.eq(guild_id.to_string()))
        .order_by_desc(temp_vc_record::Column::CreatedAt)
        .all(db)
        .await?)
}

pub async fn get_active_channel(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
) -> Result<Option<temp_vc_record::Model>> {
    Ok(temp_vc_record::Entity::find()
        .filter(temp_vc_record::Column::GuildId.eq(guild_id.to_string()))
        .filter(temp_vc_record::Column::OwnerId.eq(user_id.to_string()))
        .one(db)
        .await?)
}

pub async fn set_locked(
    db: &DatabaseConnection,
    guild_id: u64,
    channel_id: u64,
    locked: bool,
) -> Result<bool> {
    let rec = get_record(db, guild_id, channel_id).await?;
    if let Some(m) = rec {
        let mut active: temp_vc_record::ActiveModel = m.into();
        active.locked = Set(locked);
        active.update(db).await?;
        Ok(true)
    } else {
        Ok(false)
    }
}

pub async fn set_name(
    db: &DatabaseConnection,
    guild_id: u64,
    channel_id: u64,
    name: &str,
) -> Result<bool> {
    let rec = get_record(db, guild_id, channel_id).await?;
    if let Some(m) = rec {
        let mut active: temp_vc_record::ActiveModel = m.into();
        active.name = Set(name.to_string());
        active.update(db).await?;
        Ok(true)
    } else {
        Ok(false)
    }
}

pub async fn delete_record(
    db: &DatabaseConnection,
    guild_id: u64,
    channel_id: u64,
) -> Result<bool> {
    let result = temp_vc_record::Entity::delete_many()
        .filter(temp_vc_record::Column::GuildId.eq(guild_id.to_string()))
        .filter(temp_vc_record::Column::ChannelId.eq(channel_id.to_string()))
        .exec(db)
        .await?;
    Ok(result.rows_affected > 0)
}
