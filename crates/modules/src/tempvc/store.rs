use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
    sea_query::OnConflict,
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
        guild_id: Set(lumi_db::ids::GuildId::from(guild_id)),
        channel_id: Set(lumi_db::ids::ChannelId::from(channel_id)),
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
        .filter(temp_vc_generator::Column::GuildId.eq(lumi_db::ids::GuildId::from(guild_id)))
        .filter(temp_vc_generator::Column::ChannelId.eq(lumi_db::ids::ChannelId::from(channel_id)))
        .one(db)
        .await?)
}

pub async fn list_generators(
    db: &DatabaseConnection,
    guild_id: u64,
) -> Result<Vec<temp_vc_generator::Model>> {
    Ok(temp_vc_generator::Entity::find()
        .filter(temp_vc_generator::Column::GuildId.eq(lumi_db::ids::GuildId::from(guild_id)))
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
    let Some(row) = get_generator(db, guild_id, channel_id).await? else {
        return Ok(false);
    };
    let mut active: temp_vc_generator::ActiveModel = row.into();
    if let Some(name) = name {
        active.name = Set(name);
    }
    if let Some(limit) = limit {
        active.limit = Set(limit);
    }
    active.update(db).await?;
    Ok(true)
}

pub async fn delete_generator(
    db: &DatabaseConnection,
    guild_id: u64,
    channel_id: u64,
) -> Result<bool> {
    let result = temp_vc_generator::Entity::delete_many()
        .filter(temp_vc_generator::Column::GuildId.eq(lumi_db::ids::GuildId::from(guild_id)))
        .filter(temp_vc_generator::Column::ChannelId.eq(lumi_db::ids::ChannelId::from(channel_id)))
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
) -> Result<()> {
    let row = temp_vc_record::ActiveModel {
        guild_id: Set(lumi_db::ids::GuildId::from(guild_id)),
        channel_id: Set(lumi_db::ids::ChannelId::from(channel_id)),
        owner_id: Set(lumi_db::ids::UserId::from(owner_id)),
        generator_id: Set(lumi_db::ids::ChannelId::from(generator_id)),
        name: Set(name),
        number: Set(1),
        locked: Set(false),
        hidden: Set(false),
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
                temp_vc_record::Column::GeneratorId,
                temp_vc_record::Column::Name,
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
        .filter(temp_vc_record::Column::GuildId.eq(lumi_db::ids::GuildId::from(guild_id)))
        .filter(temp_vc_record::Column::ChannelId.eq(lumi_db::ids::ChannelId::from(channel_id)))
        .one(db)
        .await?)
}

pub async fn get_active_channel(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
) -> Result<Option<temp_vc_record::Model>> {
    Ok(temp_vc_record::Entity::find()
        .filter(temp_vc_record::Column::GuildId.eq(lumi_db::ids::GuildId::from(guild_id)))
        .filter(temp_vc_record::Column::OwnerId.eq(lumi_db::ids::UserId::from(user_id)))
        .order_by_desc(temp_vc_record::Column::CreatedAt)
        .one(db)
        .await?)
}

async fn update_record(
    db: &DatabaseConnection,
    guild_id: u64,
    channel_id: u64,
    apply: impl FnOnce(&mut temp_vc_record::ActiveModel),
) -> Result<bool> {
    let Some(row) = get_record(db, guild_id, channel_id).await? else {
        return Ok(false);
    };
    let mut active: temp_vc_record::ActiveModel = row.into();
    apply(&mut active);
    active.update(db).await?;
    Ok(true)
}

pub async fn set_locked(
    db: &DatabaseConnection,
    guild_id: u64,
    channel_id: u64,
    locked: bool,
) -> Result<bool> {
    update_record(db, guild_id, channel_id, |active| {
        active.locked = Set(locked);
    })
    .await
}

pub async fn set_hidden(
    db: &DatabaseConnection,
    guild_id: u64,
    channel_id: u64,
    hidden: bool,
) -> Result<bool> {
    update_record(db, guild_id, channel_id, |active| {
        active.hidden = Set(hidden);
    })
    .await
}

pub async fn set_owner(
    db: &DatabaseConnection,
    guild_id: u64,
    channel_id: u64,
    new_owner_id: u64,
) -> Result<bool> {
    update_record(db, guild_id, channel_id, |active| {
        active.owner_id = Set(lumi_db::ids::UserId::from(new_owner_id));
    })
    .await
}

pub async fn delete_record(
    db: &DatabaseConnection,
    guild_id: u64,
    channel_id: u64,
) -> Result<bool> {
    let result = temp_vc_record::Entity::delete_many()
        .filter(temp_vc_record::Column::GuildId.eq(lumi_db::ids::GuildId::from(guild_id)))
        .filter(temp_vc_record::Column::ChannelId.eq(lumi_db::ids::ChannelId::from(channel_id)))
        .exec(db)
        .await?;
    Ok(result.rows_affected > 0)
}
