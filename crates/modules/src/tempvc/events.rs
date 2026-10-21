use poise::serenity_prelude as serenity;
use serenity::all::{ChannelId, ChannelType, CreateChannel, GenericChannelId, GuildId, VoiceState};
use tracing::{error, info, warn};

use lumi_kernel::{Result, Services};
use lumi_ui::Card;

use crate::shared::truncate_chars;

use super::{store, views};

const MAX_CHANNEL_NAME: usize = 100;
/// Per-user delay between generator joins; a faster attempt is refused.
const CREATE_COOLDOWN_MS: u64 = 30_000;

fn channel_is_empty(ctx: &serenity::Context, guild_id: GuildId, channel_id: ChannelId) -> bool {
    ctx.cache
        .guild(guild_id)
        .and_then(|guild| guild.channels.get(&channel_id).cloned())
        .is_none_or(|channel| {
            channel
                .members(&ctx.cache)
                .is_ok_and(|members| members.is_empty())
        })
}

fn parent_of(
    ctx: &serenity::Context,
    guild_id: GuildId,
    channel_id: ChannelId,
) -> Option<ChannelId> {
    ctx.cache
        .guild(guild_id)
        .and_then(|guild| guild.channels.get(&channel_id).and_then(|c| c.parent_id))
}

/// Handle voice state transitions to implement the automatic TempVC lifecycle.
/// 1. If a managed channel becomes empty -> delete the channel and its record.
/// 2. If a member joins a generator channel -> create a temporary VC and move them in.
pub async fn on_voice_state_update(
    ctx: &serenity::Context,
    services: &Services,
    old: &Option<VoiceState>,
    new: &VoiceState,
) -> Result<()> {
    let Some(guild_id) = new
        .guild_id
        .or_else(|| old.as_ref().and_then(|o| o.guild_id))
    else {
        return Ok(());
    };
    let gid = guild_id.get();

    if let Some(old_state) = old {
        if let Some(old_ch_id) = old_state.channel_id {
            if old_state.channel_id != new.channel_id
                && channel_is_empty(ctx, guild_id, old_ch_id)
                && store::get_record(&services.db, gid, old_ch_id.get())
                    .await?
                    .is_some()
            {
                info!(
                    guild_id = gid,
                    channel_id = old_ch_id.get(),
                    "cleaning up empty tempvc"
                );
                if let Err(error) = ctx
                    .http
                    .delete_channel(old_ch_id.into(), Some("TempVC channel empty cleanup"))
                    .await
                {
                    warn!(%error, channel_id = old_ch_id.get(), "tempvc cleanup delete failed");
                }
                store::delete_record(&services.db, gid, old_ch_id.get()).await?;
            }
        }
    }

    let Some(new_ch_id) = new.channel_id else {
        return Ok(());
    };
    let Some(generator) = store::get_generator(&services.db, gid, new_ch_id.get()).await? else {
        return Ok(());
    };

    let user_id = new.user_id;
    let member = match new.member.clone() {
        Some(member) => member,
        None => guild_id.member(&ctx.http, user_id).await?,
    };

    let cooldown_key = format!("lumi:tempvc:cd:create:{gid}:{}", user_id.get());
    if !services
        .cache
        .claim_cooldown(&cooldown_key, CREATE_COOLDOWN_MS)
        .await?
    {
        if let Err(error) = guild_id.disconnect_member(&ctx.http, user_id).await {
            warn!(%error, user_id = user_id.get(), "tempvc cooldown disconnect failed");
        }
        if let Ok(dm) = user_id.create_dm_channel(&ctx.http).await {
            let card = Card::warning(
                "\u{23f3} Slow Down",
                format!(
                    "Wait {} seconds before creating another channel.",
                    CREATE_COOLDOWN_MS / 1000
                ),
            );
            if let Err(error) = GenericChannelId::new(dm.id.get())
                .send_message(&ctx.http, card.into_reply().message())
                .await
            {
                warn!(%error, user_id = user_id.get(), "tempvc cooldown dm failed");
            }
        }
        return Ok(());
    }

    let channel_name = truncate_chars(
        &generator.name.replace("{user}", member.display_name()),
        MAX_CHANNEL_NAME,
    );

    let mut builder = CreateChannel::new(&channel_name).kind(ChannelType::Voice);
    if let Ok(limit) = u16::try_from(generator.limit) {
        if limit > 0 {
            if let Ok(limit) = serenity::nonmax::NonMaxU16::try_from(limit) {
                builder = builder.user_limit(limit);
            }
        }
    }
    if let Some(parent) = parent_of(ctx, guild_id, new_ch_id) {
        builder = builder.category(parent);
    }

    let created = guild_id.create_channel(&ctx.http, builder).await?;
    info!(
        guild_id = gid,
        channel_id = created.id.get(),
        owner = user_id.get(),
        "created tempvc"
    );

    if let Err(error) = guild_id.move_member(&ctx.http, user_id, created.id).await {
        error!(%error, "failed to move member into newly created tempvc");
    }

    if let Err(error) = store::create_record(
        &services.db,
        gid,
        created.id.get(),
        user_id.get(),
        new_ch_id.get(),
        channel_name,
    )
    .await
    {
        error!(%error, channel_id = created.id.get(), "failed to persist tempvc record");
        if let Err(error) = GenericChannelId::new(created.id.get())
            .delete(&ctx.http, Some("TempVC record write failed"))
            .await
        {
            warn!(%error, channel_id = created.id.get(), "tempvc rollback delete failed");
        }
        return Ok(());
    }

    let card = views::panel_card(
        created.id.get(),
        user_id.get(),
        false,
        false,
        generator.limit,
    );
    if let Err(error) = GenericChannelId::new(created.id.get())
        .send_message(&ctx.http, card.message())
        .await
    {
        warn!(%error, channel_id = created.id.get(), "tempvc panel send failed");
    }

    Ok(())
}
