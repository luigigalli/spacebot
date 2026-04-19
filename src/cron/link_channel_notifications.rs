//! Link channel message notification system.
//!
//! Provides a background service that checks for unread link channel messages
//! and sends notifications to agents.
//!
//! Run via: start_link_channel_notifier(pools, links, working_memory)
//! The notifier runs every 5 minutes.

use crate::links::AgentLink;
use arc_swap::ArcSwap;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;

/// Unread message structure.
#[derive(Debug)]
struct UnreadMessage {
    id: String,
    content: String,
    sender_name: Option<String>,
    created_at: String,
}

/// Check for unread link channel messages for all agents.
/// Sends notifications via the working memory system.
pub async fn check_unread_and_notify(
    pools: &HashMap<String, SqlitePool>,
    links: &Arc<ArcSwap<Vec<AgentLink>>>,
    working_memory: &Option<Arc<crate::memory::WorkingMemoryStore>>,
) {
    let links = links.load();
    let mut agent_notifications: HashMap<String, Vec<UnreadMessage>> = HashMap::new();

    // Collect unread messages per agent
    for link in links.iter() {
        // Messages from to_agent -> from_agent's inbox
        if let Some(pool) = pools.get(&link.to_agent_id) {
            let channel = link.channel_id_for(&link.to_agent_id);
            if let Ok(Some(messages)) = check_unread(pool, &channel).await {
                if !messages.is_empty() {
                    agent_notifications
                        .entry(link.to_agent_id.clone())
                        .or_default()
                        .extend(messages);
                }
            }
        }

        // Messages from from_agent -> to_agent's inbox  
        if let Some(pool) = pools.get(&link.from_agent_id) {
            let channel = link.channel_id_for(&link.from_agent_id);
            if let Ok(Some(messages)) = check_unread(pool, &channel).await {
                if !messages.is_empty() {
                    agent_notifications
                        .entry(link.from_agent_id.clone())
                        .or_default()
                        .extend(messages);
                }
            }
        }
    }

    // Send notifications
    for (agent_id, messages) in agent_notifications {
        let count = messages.len();
        let notification = format!(
            "You have {} unread message(s) in your link channels",
            count
        );

        if let Some(wm) = working_memory {
            wm.emit(
                crate::memory::WorkingMemoryEventType::Notification,
                notification,
            )
            .importance(0.6)
            .record();
        }

        tracing::debug!(agent_id = %agent_id, count, "link channel notification sent");
    }
}

/// Check for unread messages in a link channel.
/// Returns up to 5 unread messages.
async fn check_unread(pool: &SqlitePool, channel_id: &str) -> anyhow::Result<Option<Vec<UnreadMessage>>> {
    let rows = sqlx::query(
        "SELECT id, content, sender_name, created_at FROM conversation_messages \
         WHERE channel_id = ? AND role = 'user' \
         AND content NOT LIKE '[Acknowledgment]%' \
         ORDER BY created_at DESC LIMIT 5"
    )
    .bind(channel_id)
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        Ok(None)
    } else {
        let messages: Vec<UnreadMessage> = rows
            .iter()
            .map(|row| UnreadMessage {
                id: row.get("id"),
                content: row.get("content"),
                sender_name: row.get("sender_name"),
                created_at: row.get("created_at"),
            })
            .collect();
        Ok(Some(messages))
    }
}

/// Start the link channel notification checker.
/// Runs every 5 minutes in the background.
pub async fn start_link_channel_notifier(
    pools: HashMap<String, SqlitePool>,
    links: Arc<ArcSwap<Vec<AgentLink>>>,
    working_memory: Option<Arc<crate::memory::WorkingMemoryStore>>,
) {
    use tokio::time::interval;
    use std::time::Duration;

    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(300));

        loop {
            ticker.tick().await;
            check_unread_and_notify(&pools, &links, &working_memory).await;
        }
    });
}