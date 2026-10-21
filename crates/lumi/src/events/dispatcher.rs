use poise::serenity_prelude as serenity;
use std::sync::Arc;
use tokio::sync::mpsc;

pub const QUEUE_CAPACITY: usize = 10_000;

#[derive(Debug)]
pub enum DispatchedEvent {
    Message(Box<serenity::Message>),
    MessageDelete {
        channel_id: serenity::GenericChannelId,
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

impl DispatchedEvent {
    #[must_use]
    pub const fn event_name(&self) -> &'static str {
        match self {
            Self::Message(_) => "Message",
            Self::MessageDelete { .. } => "MessageDelete",
            Self::MessageUpdate { .. } => "MessageUpdate",
            Self::MemberAddition { .. } => "MemberAddition",
            Self::MemberRemoval { .. } => "MemberRemoval",
            Self::AuditLogEntry { .. } => "AuditLogEntry",
        }
    }

    #[must_use]
    pub fn guild_id(&self) -> Option<serenity::GuildId> {
        match self {
            Self::Message(m) => m.guild_id,
            Self::MessageDelete { guild_id, .. } => *guild_id,
            Self::MessageUpdate { event } => event.message.guild_id,
            Self::MemberAddition { member } => Some(member.guild_id),
            Self::MemberRemoval { guild_id, .. } | Self::AuditLogEntry { guild_id, .. } => {
                Some(*guild_id)
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct EventDispatcher {
    sender: mpsc::Sender<DispatchedEvent>,
}

impl EventDispatcher {
    #[must_use]
    pub fn new(ctx: serenity::Context, services: Arc<lumi_kernel::Services>) -> Self {
        let (sender, receiver) = mpsc::channel::<DispatchedEvent>(QUEUE_CAPACITY);
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
        let max_capacity = self.sender.max_capacity();
        let available = self.sender.capacity();
        let used = max_capacity.saturating_sub(available);
        let event_name = event.event_name();
        let guild_id = event.guild_id().map(|g| g.get());

        if used >= (max_capacity * 4) / 5 {
            tracing::warn!(
                event = event_name,
                guild_id = ?guild_id,
                used_capacity = used,
                max_capacity = max_capacity,
                "event queue exceeds 80% capacity; high gateway backpressure"
            );
        }

        match self.sender.try_send(event) {
            Ok(()) => true,
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!(
                    event = event_name,
                    guild_id = ?guild_id,
                    max_capacity = max_capacity,
                    "event queue full; shedding event to preserve gateway stability"
                );
                false
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                tracing::warn!(
                    event = event_name,
                    guild_id = ?guild_id,
                    "event queue closed; dropping event"
                );
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

            let guild_id = message.guild_id.map(|g| g.get());
            let channel_id = message.channel_id.get();
            let author_id = message.author.id.get();

            if let Err(error) = lumi_modules::afk::on_message(ctx, services, &message).await {
                tracing::warn!(
                    %error,
                    guild_id = ?guild_id,
                    channel_id,
                    author_id,
                    "afk message handler failed"
                );
            }

            if let Err(error) = lumi_modules::leveling::on_message(ctx, services, &message).await {
                tracing::warn!(
                    %error,
                    guild_id = ?guild_id,
                    channel_id,
                    author_id,
                    "leveling message handler failed"
                );
            }
        }
        DispatchedEvent::MessageDelete {
            channel_id,
            message_id,
            guild_id,
        } => {
            let gid = guild_id.map(|g| g.get());
            let cid = channel_id.get();
            let mid = message_id.get();
            if let Err(error) = lumi_modules::logging::events::on_message_delete(
                ctx, services, channel_id, message_id, guild_id,
            )
            .await
            {
                tracing::warn!(
                    %error,
                    guild_id = ?gid,
                    channel_id = cid,
                    message_id = mid,
                    "logging message delete failed"
                );
            }
        }
        DispatchedEvent::MessageUpdate { event } => {
            if lumi_modules::filter::events::on_message_update(ctx, services, &event).await {
                return;
            }
            let guild_id = event.message.guild_id.map(|g| g.get());
            let channel_id = event.message.channel_id.get();
            let message_id = event.message.id.get();
            if let Err(error) =
                lumi_modules::logging::events::on_message_update(ctx, services, None, *event).await
            {
                tracing::warn!(
                    %error,
                    guild_id = ?guild_id,
                    channel_id,
                    message_id,
                    "logging message update failed"
                );
            }
        }
        DispatchedEvent::MemberAddition { member } => {
            let guild_id = member.guild_id.get();
            let user_id = member.user.id.get();
            if let Err(error) =
                lumi_modules::logging::events::on_member_add(ctx, services, &member).await
            {
                tracing::warn!(
                    %error,
                    guild_id,
                    user_id,
                    "logging member add failed"
                );
            }
        }
        DispatchedEvent::MemberRemoval { guild_id, user, .. } => {
            let gid = guild_id.get();
            let uid = user.id.get();
            if let Err(error) =
                lumi_modules::logging::events::on_member_remove(ctx, services, guild_id, &user)
                    .await
            {
                tracing::warn!(
                    %error,
                    guild_id = gid,
                    user_id = uid,
                    "logging member remove failed"
                );
            }
        }
        DispatchedEvent::AuditLogEntry { entry, guild_id } => {
            let gid = guild_id.get();
            let entry_id = entry.id.get();
            if let Err(error) =
                lumi_modules::security::events::on_audit_log_entry(ctx, services, &entry, guild_id)
                    .await
            {
                tracing::warn!(
                    %error,
                    guild_id = gid,
                    entry_id,
                    "security audit log handler failed"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_metadata() {
        let event = DispatchedEvent::MessageDelete {
            channel_id: serenity::GenericChannelId::new(1),
            message_id: serenity::MessageId::new(2),
            guild_id: Some(serenity::GuildId::new(12345)),
        };
        assert_eq!(event.event_name(), "MessageDelete");
        assert_eq!(event.guild_id(), Some(serenity::GuildId::new(12345)));
    }

    #[test]
    fn drops_event_on_full_channel() {
        let (sender, _receiver) = mpsc::channel::<DispatchedEvent>(1);
        let dispatcher = EventDispatcher { sender };
        let event1 = DispatchedEvent::MessageDelete {
            channel_id: serenity::GenericChannelId::new(1),
            message_id: serenity::MessageId::new(2),
            guild_id: Some(serenity::GuildId::new(123)),
        };
        let event2 = DispatchedEvent::MessageDelete {
            channel_id: serenity::GenericChannelId::new(3),
            message_id: serenity::MessageId::new(4),
            guild_id: Some(serenity::GuildId::new(123)),
        };
        assert!(dispatcher.dispatch(event1));
        assert!(!dispatcher.dispatch(event2));
    }
}
