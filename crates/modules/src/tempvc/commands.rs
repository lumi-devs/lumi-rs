use poise::serenity_prelude as serenity;
use serenity::all::{ChannelId, PermissionOverwrite, PermissionOverwriteType, Permissions};

use lumi_kernel::bot::{Command, Context, Error};
use super::store;
use super::views::{
    channel_limit_card, channel_locked_card, channel_renamed_card, error_card,
    generator_deleted_card, generator_setup_card,
};

/// Configure or manage dynamic temporary voice channel generators.
#[poise::command(
    slash_command,
    guild_only,
    subcommands("tempvc_setup", "tempvc_delete"),
    default_member_permissions = "MANAGE_CHANNELS"
)]
pub async fn tempvc(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Setup a voice channel as a generator for temporary voice channels.
#[poise::command(slash_command, rename = "setup")]
pub async fn tempvc_setup(
    ctx: Context<'_>,
    #[description = "The voice channel to act as generator"] channel: ChannelId,
    #[description = "Template name for generated channels (e.g. {user}'s VC)"] name: Option<String>,
    #[description = "Default user limit for generated channels (0 for unlimited)"] limit: Option<i32>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?.get();
    let template_name = name.unwrap_or_else(|| "{user}'s Channel".to_string());
    let user_limit = limit.unwrap_or(0);

    store::create_generator(
        &ctx.data().db,
        guild_id,
        channel.get(),
        template_name.clone(),
        user_limit,
    )
    .await?;

    let card = generator_setup_card(channel.get(), &template_name, user_limit);
    ctx.send(card.reply()).await?;
    Ok(())
}

/// Remove a temporary voice channel generator.
#[poise::command(slash_command, rename = "delete")]
pub async fn tempvc_delete(
    ctx: Context<'_>,
    #[description = "The generator voice channel to remove"] channel: ChannelId,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?.get();

    let deleted = store::delete_generator(&ctx.data().db, guild_id, channel.get()).await?;
    if deleted {
        let card = generator_deleted_card(channel.get());
        ctx.send(card.reply()).await?;
    } else {
        let card = error_card("That channel is not registered as a Temporary VC generator.");
        ctx.send(card.reply()).await?;
    }
    Ok(())
}

/// Manage your temporary voice channel.
#[poise::command(
    slash_command,
    guild_only,
    subcommands("vc_name", "vc_limit", "vc_lock")
)]
pub async fn vc(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Rename your temporary voice channel.
#[poise::command(slash_command, rename = "name")]
pub async fn vc_name(
    ctx: Context<'_>,
    #[description = "New name for your voice channel"] name: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?.get();
    let user_id = ctx.author().id.get();

    let record = store::get_active_channel(&ctx.data().db, guild_id, user_id).await?;
    let Some(rec) = record else {
        let card = error_card("You do not own an active temporary voice channel.");
        ctx.send(card.reply()).await?;
        return Ok(());
    };

    let channel_id = ChannelId::new(rec.channel_id.parse::<u64>()?);
    let builder = serenity::EditChannel::new().name(&name);
    channel_id.edit(ctx.http(), builder).await?;

    store::set_name(&ctx.data().db, guild_id, channel_id.get(), &name).await?;

    let card = channel_renamed_card(&name);
    ctx.send(card.reply()).await?;
    Ok(())
}

/// Change user limit of your temporary voice channel.
#[poise::command(slash_command, rename = "limit")]
pub async fn vc_limit(
    ctx: Context<'_>,
    #[description = "User limit (0 for unlimited)"] limit: u32,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?.get();
    let user_id = ctx.author().id.get();

    let record = store::get_active_channel(&ctx.data().db, guild_id, user_id).await?;
    let Some(rec) = record else {
        let card = error_card("You do not own an active temporary voice channel.");
        ctx.send(card.reply()).await?;
        return Ok(());
    };

    let channel_id = ChannelId::new(rec.channel_id.parse::<u64>()?);
    let builder = serenity::EditChannel::new().user_limit(limit);
    channel_id.edit(ctx.http(), builder).await?;

    let card = channel_limit_card(limit as i32);
    ctx.send(card.reply()).await?;
    Ok(())
}

/// Lock or unlock your temporary voice channel.
#[poise::command(slash_command, rename = "lock")]
pub async fn vc_lock(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?;
    let user_id = ctx.author().id.get();

    let record = store::get_active_channel(&ctx.data().db, guild_id.get(), user_id).await?;
    let Some(rec) = record else {
        let card = error_card("You do not own an active temporary voice channel.");
        ctx.send(card.reply()).await?;
        return Ok(());
    };

    let channel_id = ChannelId::new(rec.channel_id.parse::<u64>()?);
    let new_locked = !rec.locked;

    let everyone_role_id = guild_id.everyone_role();
    let permissions = if new_locked {
        PermissionOverwrite {
            allow: Permissions::empty(),
            deny: Permissions::CONNECT,
            kind: PermissionOverwriteType::Role(everyone_role_id),
        }
    } else {
        PermissionOverwrite {
            allow: Permissions::CONNECT,
            deny: Permissions::empty(),
            kind: PermissionOverwriteType::Role(everyone_role_id),
        }
    };

    let builder = serenity::EditChannel::new().permissions(vec![permissions]);
    channel_id.edit(ctx.http(), builder).await?;

    store::set_locked(&ctx.data().db, guild_id.get(), channel_id.get(), new_locked).await?;

    let card = channel_locked_card(new_locked);
    ctx.send(card.reply()).await?;
    Ok(())
}

pub fn commands() -> Vec<Command> {
    vec![tempvc(), vc()]
}
