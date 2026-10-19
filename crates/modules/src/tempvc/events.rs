use poise::serenity_prelude as serenity;
use serenity::all::{ChannelType, CreateChannel, VoiceState};
use tracing::{error, info};

use lumi_kernel::{Result, Services};
use super::store;

/// Handle voice state transitions to implement the automatic TempVC lifecycle.
/// 1. If user leaves a managed channel and it becomes empty -> delete the channel & record.
/// 2. If user joins a generator channel -> create a new temporary VC, assign permissions, and move user.
pub async fn on_voice_state_update(
    ctx: &serenity::Context,
    services: &Services,
    old: &Option<VoiceState>,
    new: &VoiceState,
) -> Result<()> {
    let Some(guild_id) = new.guild_id.or_else(|| old.as_ref().and_then(|o| o.guild_id)) else {
        return Ok(());
    };
    let gid = guild_id.get();

    // 1. Check if user left a managed voice channel
    if let Some(old_state) = old {
        if let Some(old_ch_id) = old_state.channel_id {
            if old_state.channel_id != new.channel_id {
                let rec = store::get_record(&services.db, gid, old_ch_id.get()).await?;
                if rec.is_some() {
                    // Check if channel is empty
                    let is_empty = match guild_id.channels(&ctx.http).await {
                        Ok(channels) => {
                            if let Some(ch) = channels.get(&old_ch_id) {
                                match ch.members(&ctx.cache) {
                                    Ok(members) => members.is_empty(),
                                    Err(_) => true,
                                }
                            } else {
                                true
                            }
                        }
                        Err(_) => false,
                    };

                    if is_empty {
                        info!(guild_id = gid, channel_id = old_ch_id.get(), "cleaning up empty tempvc");
                        let _ = ctx.http.delete_channel(old_ch_id.into(), Some("TempVC channel empty cleanup")).await;
                        let _ = store::delete_record(&services.db, gid, old_ch_id.get()).await;
                    }
                }
            }
        }
    }

    // 2. Check if user joined a generator channel
    let Some(new_ch_id) = new.channel_id else {
        return Ok(());
    };

    let user_id = new.user_id.get();
    let gen_opt = store::get_generator(&services.db, gid, new_ch_id.get()).await?;
    let Some(generator) = gen_opt else {
        return Ok(());
    };

    // User joined a generator channel -> create temporary VC for them
    let member = match new.member.clone() {
        Some(m) => m,
        None => guild_id.member(&ctx.http, new.user_id).await?,
    };

    let member_name = member.display_name().to_string();
    let channel_name = generator.name.replace("{user}", &member_name);

    // Get parent category from generator channel if available
    let parent_id = match guild_id.channels(&ctx.http).await {
        Ok(chs) => chs.get(&new_ch_id).and_then(|c| c.parent_id),
        Err(_) => None,
    };

    let mut builder = CreateChannel::new(&channel_name).kind(ChannelType::Voice);

    if generator.limit > 0 {
        if let Ok(limit) = serenity::nonmax::NonMaxU16::try_from(generator.limit as u16) {
            builder = builder.user_limit(limit);
        }
    }

    if let Some(parent) = parent_id {
        builder = builder.category(parent);
    }

    let created = guild_id.create_channel(&ctx.http, builder).await?;
    info!(guild_id = gid, channel_id = created.id.get(), owner = user_id, "created tempvc");

    // Move user to the created voice channel
    if let Err(e) = guild_id.move_member(&ctx.http, new.user_id, created.id).await {
        error!(error = %e, "failed to move member into newly created tempvc");
    }

    // Record creation in DB
    let _ = store::create_record(
        &services.db,
        gid,
        created.id.get(),
        user_id,
        new_ch_id.get(),
        channel_name,
        1,
        false,
        false,
    ).await;

    Ok(())
}
