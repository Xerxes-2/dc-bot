use chrono::TimeDelta;
use futures::TryStreamExt;
use serde_json::json;
use serenity::all::*;
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};
use tokio::{spawn, sync::RwLock, task::JoinHandle};
use tracing::{error, warn};

use crate::{config::BOT_CONFIG, error::BotError};

#[derive(Default)]
pub struct TreeHoleHandler {
    msgs: RwLock<HashMap<MessageId, JoinHandle<()>>>,
}

#[async_trait]
impl EventHandler for TreeHoleHandler {
    // Set a handler for the `message` event. This is called whenever a new message is received.
    //
    // Event handlers are dispatched through a threadpool, and so multiple events can be
    // dispatched simultaneously.
    async fn message(&self, ctx: Context, msg: Message) {
        let channel_id = msg.channel_id;
        let Some(dur) = BOT_CONFIG.load().tree_holes.get(&channel_id).cloned() else {
            return; // Not a tree hole channel, ignore the message
        };
        let msg_id = msg.id;
        // await dur then delete the message
        let h = spawn(async move {
            tokio::time::sleep(dur).await;
            if let Err(err) = msg.delete(&ctx.http).await {
                error!("Failed to delete message {}: {}", msg.id, err);
            }
        });
        // Store the handle in the map
        let mut msgs = self.msgs.write().await;
        msgs.insert(msg_id, h);
        // clean up aborted tasks
        msgs.retain(|_, handle| !handle.is_finished());
    }

    async fn channel_pins_update(&self, ctx: Context, event: ChannelPinsUpdateEvent) {
        if !BOT_CONFIG.load().tree_holes.contains_key(&event.channel_id) {
            return; // Not a tree hole channel, ignore the message
        };
        if event.last_pin_timestamp.is_none() {
            return; // No pins, nothing to do
        };
        // get pinned messages
        let Ok(pinned_messages) = event.channel_id.pins(ctx.to_owned()).await else {
            warn!(
                "Failed to fetch pinned messages for channel {}",
                event.channel_id
            );
            return; // If we can't fetch pinned messages, we can't do anything
        };
        {
            let mut msgs = self.msgs.write().await;
            pinned_messages
                .into_iter()
                .filter_map(|msg| msgs.remove(&msg.id))
                .for_each(|handle| handle.abort());
        }
        self.delete_messages(ctx).await;
    }

    async fn cache_ready(&self, ctx: Context, _guilds: Vec<GuildId>) {
        self.delete_messages(ctx).await;
    }

    async fn resume(&self, ctx: Context, _resumed: ResumedEvent) {
        self.delete_messages(ctx).await;
    }
}

impl TreeHoleHandler {
    async fn delete_in_channel(
        &self,
        ctx: Context,
        channel_id: ChannelId,
        dur: Duration,
    ) -> Result<(), BotError> {
        let messages = channel_id
            .messages_iter(ctx.to_owned())
            .try_collect::<Vec<_>>()
            .await?;

        let keys = self
            .msgs
            .read()
            .await
            .keys()
            .cloned()
            .collect::<HashSet<_>>();
        let mut old = Vec::new();
        for msg in messages
            .into_iter()
            .filter(|msg| !keys.contains(&msg.id) && !msg.pinned)
        {
            let ctx = ctx.to_owned();
            let now = chrono::Utc::now();
            let delta = TimeDelta::from_std(dur).unwrap();
            let new_dur = delta - (now - msg.timestamp.to_utc());
            let msg_id = msg.id;
            if new_dur > chrono::Duration::zero() {
                let h = spawn(async move {
                    tokio::time::sleep(new_dur.to_std().unwrap()).await;
                    if let Err(err) = msg.delete(ctx).await {
                        error!("Failed to delete message {}: {}", msg.id, err);
                    }
                });
                let mut msgs = self.msgs.write().await;
                msgs.insert(msg_id, h);
            } else {
                old.push(msg.id);
            }
        }
        for chunk in old.chunks(100) {
            if let [m] = chunk {
                // If there's only one message, we can use the simpler delete_message method
                if let Err(e) = ctx.http.delete_message(channel_id, *m, None).await {
                    warn!("Failed to delete message {m} in tree hole channel {channel_id}: {e}");
                }
                continue;
            }
            if let Err(e) = ctx
                .http
                .delete_messages(channel_id, &json!({"messages": chunk}), None)
                .await
            {
                warn!("Failed to delete messages in tree hole channel {channel_id}: {e}");
            }
        }
        Ok(())
    }

    async fn delete_messages(&self, ctx: Context) {
        for (channel_id, dur) in BOT_CONFIG.load().tree_holes.iter() {
            if let Err(e) = self
                .delete_in_channel(ctx.to_owned(), *channel_id, *dur)
                .await
            {
                error!("Failed to delete messages in tree hole channel {channel_id}: {e}");
            }
        }
    }
}
