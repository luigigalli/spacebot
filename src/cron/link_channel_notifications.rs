//! Link channel message notification system.
//!
//! Provides a background service that checks for unread link channel messages
//! and sends actual portal messages to agents when they have unread messages.
//!
//! Run via: start_link_channel_notifier(pools, links, injection_tx)
//! The notifier runs every 5 minutes.

use crate::links::AgentLink;
use crate::{ChannelInjection, InboundMessage, MessageContent};
use arc_swap::ArcSwap;
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Unread message structure.
#[derive(Debug, Clone)]
struct UnreadMessage {
    id: String,
    content: String,
    sender_name: Option<String>,
    created_at: String,
    channel_id: String,
}

/// Main notification function - checks for unread and injects messages to agents.
pub async fn check_unread_and_notify(
    pools: &HashMap<String, SqlitePool>,
    links: &Arc<ArcSwap<Vec<AgentLink>>>,
    injection_tx: &mpsc::Sender<ChannelInjection>,
) {
    let links = links.load();
    let mut agent_notifications: HashMap<String, Vec<UnreadMessage>> = HashMap::new();

    // Collect unread messages per agent
    for link in links.iter() {
        // Check to_agent side (messages from from_agent)
        if let Some(pool) = pools.get(&link.to_agent_id) {
            let channel = link.channel_id_for(&link.from_agent_id);
            if let Ok(Some(messages)) = check_unread(pool, &channel).await {
                if !messages.is_empty() {
                    agent_notifications
                        .entry(link.to_agent_id.clone())
                        .or_default()
                        .extend(messages);
                }
            }
        }

        // Check from_agent side (messages from to_agent)
        if let Some(pool) = pools.get(&link.from_agent_id) {
            let channel = link.channel_id_for(&link.to_agent_id);
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

    // Send portal messages to each agent with unread messages
    for (agent_id, messages) in agent_notifications {
        let count = messages.len();
        
        // Group by sender
        let mut by_sender: HashMap<String, Vec<String>> = HashMap::new();
        for msg in &messages {
            let sender = msg.sender_name.clone().unwrap_or_else(|| "unknown".to_string());
            by_sender.entry(sender).or_default().push(msg.content.clone());
        }

        // Build notification message showing who from which channel
        let mut notification = format!("📬 You have {} unread message(s) from link channel(s):\n\n", count);
        for (sender, msgs) in &by_sender {
            let preview = if msgs[0].len() > 50 { 
                format!("{}...", &msgs[0][..50]) 
            } else { 
                msgs[0].clone() 
            };
            notification.push_str(&format!("• From {}: {}\n", sender, preview));
        }
        
        // Create portal channel ID
        let portal_channel = format!("portal:link-notify:{}", agent_id);

        // Create and inject the notification
        let inbound = InboundMessage {
            id: uuid::Uuid::new_v4().to_string(),
            conversation_id: portal_channel.clone(),
            channel_id: portal_channel.clone(),
            sender_id: "link-channel-notifier".to_string(),
            author_id: "link-channel-notifier".to_string(),
            content: MessageContent::Text(notification.clone()),
            content_plain: Some(notification.clone()),
            formatted_author: Some("Link Channel Notifier".to_string()),
            source: "system".to_string(),
            reply_to: None,
            thread_ts: None,
            metadata: HashMap::new(),
        };

        let injection = ChannelInjection {
            conversation_id: portal_channel,
            agent_id: agent_id.clone(),
            message: inbound,
        };

        if let Err(e) = injection_tx.send(injection).await {
            tracing::warn!(agent_id = %agent_id, error = %e, "failed to inject notification");
        } else {
            tracing::info!(agent_id = %agent_id, count, "link channel notification sent");
        }
    }
}

/// Check for unread messages in a link channel.
/// Returns up to 5 unread messages (messages that don't have [Acknowledgment] prefix).
async fn check_unread(pool: &SqlitePool, channel_id: &str) -> anyhow::Result<Option<Vec<UnreadMessage>>> {
    let rows = sqlx::query(
        "SELECT id, content, sender_name, created_at, channel_id FROM conversation_messages \
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
                channel_id: row.get("channel_id"),
            })
            .collect();
        Ok(Some(messages))
    }
}