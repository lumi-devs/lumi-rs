use std::sync::Arc;
use poise::serenity_prelude as serenity;
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum DispatchedEvent {
    Message(Box<serenity::Message>),
    MessageDelete {
        channel_id: serenity::ChannelId,
        message_id: serenity::MessageId,
        guild_id: Option<serenity::GuildId>,
    },
    MessageUpdate {
        event: Box<serenity::MessageUpdateEvent>,
    },
    MemberAddition {
        member: Box<serenity::Member>,
    },
    MemberRemoval {
        guild_id: serenity::GuildId,
        user: serenity::User,
        member_data_if_available: Option<Box<serenity::Member>>,
    },
    AuditLogEntry {
        entry: Box<serenity::AuditLogEntry>,
        guild_id: serenity::GuildId,
    },
}

#[derive(Clone, Debug)]
pub struct EventDispatcher {
    sender: mpsc::Sender<DispatchedEvent>,
}

impl EventDispatcher {
    #[must_use]
    pub fn new(ctx: serenity::Context, services: Arc<lumi_kernel::Services>) -> Self {
        let (sender, receiver) = mpsc::channel::<DispatchedEvent>(10_000);
        let receiver = Arc::new(tokio::sync::Mutex::new(receiver));

        for worker_id in 0..8 {
            let rx = Arc::clone(&receiver);
            let ctx = ctx.clone();
            let services = Arc::clone(&services);

            tokio::spawn(async move {
                loop {
                    let event = {
                        let mut guard = rx.lock().await;
                        guard.recv().await
                    };

                    let Some(event) = event else {
                        break;
                    };

                    process_event(&ctx, &services, event, worker_id).await;
                }
            });
        }

        Self { sender }
    }

    pub fn dispatch(&self, event: DispatchedEvent) -> bool {
        match self.sender.try_send(event) {
            Ok(()) => true,
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!("event queue full (10000 capacity reached); shedding event to preserve gateway stability");
                false
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                tracing::warn!("event queue closed; dropping event");
                false
            }
        }
    }
}

async fn process_event(
    ctx: &serenity::Context,
    services: &lumi_kernel::Services,
    event: DispatchedEvent,
    _worker_id: usize,
) {
    match event {
        DispatchedEvent::Message(message) => {
            if lumi_modules::filter::events::on_message(ctx, services, &message).await {
                return;
            }

            if let Err(error) = lumi_modules::afk::on_message(ctx, services, &message).await {
                tracing::warn!(%error, "afk message handler failed");
            }

            if let Err(error) = lumi_modules::leveling::on_message(ctx, services, &message).await {
                tracing::warn!(%error, "leveling message handler failed");
            }
        }
        DispatchedEvent::MessageDelete {
            channel_id,
            message_id,
            guild_id,
        } => {
            if let Err(error) = lumi_modules::logging::events::on_message_delete(
                ctx,
                services,
                serenity::GenericChannelId::new(channel_id.get()),
                message_id,
                guild_id,
            )
            .await
            {
                tracing::warn!(%error, "logging message delete failed");
            }
        }
        DispatchedEvent::MessageUpdate { event } => {
            if let Err(error) = lumi_modules::logging::events::on_message_update(
                ctx,
                services,
                None,
                *event,
            )
            .await
            {
                tracing::warn!(%error, "logging message update failed");
            }
        }
        DispatchedEvent::MemberAddition { member } => {
            if let Err(error) =
                lumi_modules::logging::events::on_member_add(ctx, services, &member).await
            {
                tracing::warn!(%error, "logging member add failed");
            }
        }
        DispatchedEvent::MemberRemoval { guild_id, user, .. } => {
            if let Err(error) =
                lumi_modules::logging::events::on_member_remove(ctx, services, guild_id, &user).await
            {
                tracing::warn!(%error, "logging member remove failed");
            }
        }
        DispatchedEvent::AuditLogEntry { entry, guild_id } => {
            if let Err(error) =
                lumi_modules::security::events::on_audit_log_entry(ctx, services, &entry, guild_id)
                    .await
            {
                tracing::warn!(%error, "security audit log handler failed");
            }
        }
    }
}
