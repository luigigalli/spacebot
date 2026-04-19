//! API handlers for link channel (agent-to-agent) message reading and writing.

use crate::ChannelId;
use crate::api::state::{ApiEvent, ApiState};
use crate::conversation::history::{ConversationLogger, ConversationMessage};
use crate::links::LinkDirection;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Sse;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(super) struct LinkChannelMessage {
    pub id: String,
    pub channel_id: String,
    pub role: String,
    pub sender_name: Option<String>,
    pub sender_id: Option<String>,
    pub content: String,
    pub metadata: Option<String>,
    pub created_at: String,
}

impl From<ConversationMessage> for LinkChannelMessage {
    fn from(msg: ConversationMessage) -> Self {
        Self {
            id: msg.id,
            channel_id: msg.channel_id,
            role: msg.role,
            sender_name: msg.sender_name,
            sender_id: msg.sender_id,
            content: msg.content,
            metadata: msg.metadata,
            created_at: msg.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(super) struct LinkMessagesResponse {
    pub messages: Vec<LinkChannelMessage>,
    pub has_more: bool,
}

#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub(super) struct LinkMessagesQuery {
    /// Link channel ID (e.g., "link:orchestrator:nexus")
    channel_id: String,
    /// Maximum number of messages to return (default: 20, max: 100)
    #[serde(default = "default_message_limit")]
    limit: i64,
    /// Filter by message role: "user", "assistant", or "system"
    role: Option<String>,
    /// Filter by sender ID or sender name
    sender: Option<String>,
    /// Return messages before this RFC 3339 timestamp (for pagination)
    before: Option<String>,
    /// Return messages after this RFC 3339 timestamp
    after: Option<String>,
    /// If true, return oldest first; otherwise newest first (default: false)
    #[serde(default)]
    oldest_first: bool,
}

fn default_message_limit() -> i64 {
    20
}

/// Validate that a channel ID is a valid link channel.
fn is_valid_link_channel(channel_id: &str) -> bool {
    let parts: Vec<&str> = channel_id.split(':').collect();
    parts.len() == 3 && parts[0] == "link" && !parts[1].is_empty() && !parts[2].is_empty()
}

/// Validate sender filter (allow alphanumeric, dash, underscore).
fn is_valid_sender(sender: &str) -> bool {
    !sender.is_empty()
        && sender
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '@')
}

/// List messages from a link channel with optional filtering.
#[utoipa::path(
    get,
    path = "/link-channels/messages",
    params(
        ("channel_id" = String, Query, description = "Link channel ID (e.g., link:orchestrator:nexus)"),
        ("limit" = i64, Query, description = "Maximum number of messages (default: 20, max: 100)"),
        ("role" = Option<String>, Query, description = "Filter by role: user, assistant, or system"),
        ("sender" = Option<String>, Query, description = "Filter by sender ID or name"),
        ("before" = Option<String>, Query, description = "Return messages before this RFC 3339 timestamp"),
        ("after" = Option<String>, Query, description = "Return messages after this RFC 3339 timestamp"),
        ("oldest_first" = bool, Query, description = "If true, return oldest first (default: false)"),
    ),
    responses(
        (status = 200, body = LinkMessagesResponse),
        (status = 400, description = "Invalid request parameters"),
        (status = 403, description = "Access denied to this link channel"),
        (status = 404, description = "Link channel not found"),
    ),
    tag = "link-channels",
)]
pub(super) async fn list_link_channel_messages(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<LinkMessagesQuery>,
) -> Result<Json<LinkMessagesResponse>, StatusCode> {
    let channel_id = &query.channel_id;

    // Validate channel ID format
    if !is_valid_link_channel(channel_id) {
        tracing::warn!(%channel_id, "invalid link channel ID format");
        return Err(StatusCode::BAD_REQUEST);
    }

    // Validate sender filter
    if let Some(sender) = &query.sender {
        if !is_valid_sender(sender) {
            tracing::warn!(sender = %sender, "invalid sender filter");
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    // Validate role filter
    if let Some(role) = &query.role {
        match role.as_str() {
            "user" | "assistant" | "system" => {}
            _ => {
                tracing::warn!(role = %role, "invalid role filter");
                return Err(StatusCode::BAD_REQUEST);
            }
        }
    }

    let limit = query.limit.min(100).max(1);

    // Get any agent pool to use for loading messages
    // Link channels are instance-level, not per-agent
    let pools = state.agent_pools.load();

    let pool = pools.values().next().ok_or_else(|| {
        tracing::warn!("no agent pools available for link channel read");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let logger = ConversationLogger::new(pool.clone());

    // Load messages from the link channel
    let messages = logger
        .load_channel_transcript(
            channel_id,
            limit + 1, // Fetch one extra to check has_more
            query.before.as_deref(),
            query.after.as_deref(),
            query.oldest_first,
        )
        .await
        .map_err(|error| {
            tracing::warn!(%error, %channel_id, "failed to load link channel messages");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Check if there are more messages
    let has_more = messages.len() as i64 > limit;

    let messages: Vec<LinkChannelMessage> = if has_more {
        if query.oldest_first {
            messages[..limit as usize].to_vec()
        } else {
            messages[1..].to_vec() // Skip the extra one in newest-first mode
        }
    } else {
        messages
    }
    .into_iter()
    .map(LinkChannelMessage::from)
    .collect();

    // Apply in-memory filters (role and sender) if specified
    let filtered_messages: Vec<LinkChannelMessage> = messages
        .into_iter()
        .filter(|msg| {
            // Filter by role
            if let Some(ref role) = query.role {
                if msg.role != *role {
                    return false;
                }
            }
            // Filter by sender
            if let Some(ref sender) = query.sender {
                let sender_match = msg
                    .sender_id
                    .as_ref()
                    .map(|id| id.contains(sender))
                    .unwrap_or(false)
                    || msg
                        .sender_name
                        .as_ref()
                        .map(|name| name.to_lowercase().contains(&sender.to_lowercase()))
                        .unwrap_or(false);
                if !sender_match {
                    return false;
                }
            }
            true
        })
        .collect();

    Ok(Json(LinkMessagesResponse {
        messages: filtered_messages,
        has_more,
    }))
}

// --- List available link channels ---

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(super) struct LinkChannelInfo {
    pub channel_id: String,
    pub from_agent: String,
    pub to_agent: String,
    pub link_kind: String,
    pub direction: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(super) struct LinkChannelsListResponse {
    pub channels: Vec<LinkChannelInfo>,
}

/// List all available link channels in the instance.
#[utoipa::path(
    get,
    path = "/link-channels",
    responses(
        (status = 200, body = LinkChannelsListResponse),
    ),
    tag = "link-channels",
)]
pub(super) async fn list_link_channels(
    State(state): State<Arc<ApiState>>,
) -> Json<LinkChannelsListResponse> {
    let all_links = state.agent_links.load();

    let channels: Vec<LinkChannelInfo> = all_links
        .iter()
        .flat_map(|link| {
            let from_channel_id = link.channel_id_for(&link.from_agent_id);
            let to_channel_id = link.channel_id_for(&link.to_agent_id);

            vec![
                LinkChannelInfo {
                    channel_id: from_channel_id,
                    from_agent: link.from_agent_id.clone(),
                    to_agent: link.to_agent_id.clone(),
                    link_kind: link.kind.as_str().to_string(),
                    direction: link.direction.as_str().to_string(),
                },
                LinkChannelInfo {
                    channel_id: to_channel_id,
                    from_agent: link.to_agent_id.clone(),
                    to_agent: link.from_agent_id.clone(),
                    link_kind: link.kind.as_str().to_string(),
                    direction: link.direction.as_str().to_string(),
                },
            ]
        })
        .collect();

    Json(LinkChannelsListResponse { channels })
}

// --- Send message to link channel ---

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(super) struct SendLinkMessageRequest {
    /// Link channel ID (e.g., "link:orchestrator:nexus")
    pub channel_id: String,
    /// Message content
    pub content: String,
    /// Message type: "text" (default), "task_delegation", "acknowledgment", "status_update"
    #[serde(default = "default_message_type")]
    pub message_type: String,
}

fn default_message_type() -> String {
    "text".to_string()
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(super) struct SendLinkMessageResponse {
    pub success: bool,
    pub message_id: String,
    pub channel_id: String,
    pub delivered_to: Option<String>,
}

/// Send a message to a link channel.
///
/// This allows agents to communicate directly through their link channel.
/// The message is logged in both directions of the link channel.
#[utoipa::path(
    post,
    path = "/link-channels/messages",
    request_body = SendLinkMessageRequest,
    responses(
        (status = 201, body = SendLinkMessageResponse),
        (status = 400, description = "Invalid request parameters"),
        (status = 403, description = "Access denied or link direction prevents sending"),
        (status = 404, description = "Link channel not found"),
    ),
    tag = "link-channels",
)]
pub(super) async fn send_link_channel_message(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<SendLinkMessageRequest>,
) -> Result<Json<SendLinkMessageResponse>, StatusCode> {
    let channel_id = &request.channel_id;

    // Validate channel ID format
    if !is_valid_link_channel(channel_id) {
        tracing::warn!(%channel_id, "invalid link channel ID format");
        return Err(StatusCode::BAD_REQUEST);
    }

    // Validate message type
    match request.message_type.as_str() {
        "text" | "task_delegation" | "acknowledgment" | "status_update" => {}
        _ => {
            tracing::warn!(message_type = %request.message_type, "invalid message type");
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    // Validate content
    if request.content.trim().is_empty() {
        tracing::warn!("empty message content");
        return Err(StatusCode::BAD_REQUEST);
    }

    // Parse the channel ID to find the agents
    let parts: Vec<&str> = channel_id.split(':').collect();
    let from_agent_id = parts[1];
    let to_agent_id = parts[2];

    // Check if the link exists and allows sending
    let all_links = state.agent_links.load();
    let link = all_links
        .iter()
        .find(|l| {
            (l.from_agent_id == from_agent_id && l.to_agent_id == to_agent_id)
                || (l.from_agent_id == to_agent_id && l.to_agent_id == from_agent_id)
        })
        .ok_or_else(|| {
            tracing::warn!(from = from_agent_id, to = to_agent_id, "link not found");
            StatusCode::NOT_FOUND
        })?;

    // Check direction: from_agent must be the sender
    let sending_allowed =
        link.from_agent_id == from_agent_id || link.direction == LinkDirection::TwoWay;

    if !sending_allowed {
        tracing::warn!(
            from = from_agent_id,
            to = to_agent_id,
            direction = ?link.direction,
            "send not allowed in this direction"
        );
        return Err(StatusCode::FORBIDDEN);
    }

    // Get agent name for display
    let agent_configs = state.agent_configs.load();
    let from_display = agent_configs
        .iter()
        .find(|c| c.id == from_agent_id)
        .and_then(|c| c.display_name.clone())
        .unwrap_or_else(|| from_agent_id.to_string());

    // Get pool for logging
    let pools = state.agent_pools.load();
    let pool = pools.values().next().ok_or_else(|| {
        tracing::warn!("no agent pools available for link channel write");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let logger = ConversationLogger::new(pool.clone());

    // Generate message ID
    let message_id = uuid::Uuid::new_v4().to_string();

    // Build metadata with message type as a HashMap (clone to avoid moving)
    let mut metadata = HashMap::new();
    metadata.insert(
        "message_type".to_string(),
        serde_json::Value::String(request.message_type.clone()),
    );

    // Store message type for logging after metadata is used
    let message_type = request.message_type;

    // Create ChannelId from the channel_id string
    let sender_channel_id: ChannelId = channel_id.as_str().into();
    let reverse_channel_id = format!("link:{}:{}", to_agent_id, from_agent_id);
    let receiver_channel_id: ChannelId = reverse_channel_id.as_str().into();

    // Log message in the sender's link channel
    logger.log_user_message(
        &sender_channel_id,
        &from_display,
        from_agent_id,
        &request.content,
        &metadata,
    );

    // Also log in the receiver's link channel (reverse direction)
    logger.log_user_message(
        &receiver_channel_id,
        &from_display,
        from_agent_id,
        &request.content,
        &metadata,
    );

    tracing::info!(
        from = from_agent_id,
        to = to_agent_id,
        message_id = %message_id,
        message_type = %message_type,
        "link channel message sent"
    );

    // Broadcast event for real-time notification
    let event = crate::api::state::ApiEvent::AgentMessageSent {
        from_agent_id: from_agent_id.to_string(),
        to_agent_id: to_agent_id.to_string(),
        link_id: format!("{}->{}", from_agent_id, to_agent_id),
        channel_id: channel_id.to_string(),
    };
    state.send_event(event);

    Ok(Json(SendLinkMessageResponse {
        success: true,
        message_id,
        channel_id: channel_id.to_string(),
        delivered_to: Some(to_agent_id.to_string()),
    }))
}

// --- Acknowledge message delivery ---

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(super) struct AcknowledgeMessageRequest {
    /// Original message ID to acknowledge
    pub message_id: String,
    /// Link channel ID
    pub channel_id: String,
    /// Status: "delivered", "read", "processed"
    #[serde(default = "default_ack_status")]
    pub status: String,
}

fn default_ack_status() -> String {
    "delivered".to_string()
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(super) struct AcknowledgeMessageResponse {
    pub success: bool,
    pub message_id: String,
    pub acknowledged_at: String,
}

/// Acknowledge receipt of a link channel message.
///
/// This confirms delivery and allows the sender to know the message was received.
#[utoipa::path(
    post,
    path = "/link-channels/messages/acknowledge",
    request_body = AcknowledgeMessageRequest,
    responses(
        (status = 200, body = AcknowledgeMessageResponse),
        (status = 400, description = "Invalid request parameters"),
        (status = 404, description = "Message not found"),
    ),
    tag = "link-channels",
)]
pub(super) async fn acknowledge_link_message(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<AcknowledgeMessageRequest>,
) -> Result<Json<AcknowledgeMessageResponse>, StatusCode> {
    let channel_id = &request.channel_id;

    // Validate channel ID format
    if !is_valid_link_channel(channel_id) {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Validate status
    match request.status.as_str() {
        "delivered" | "read" | "processed" => {}
        _ => return Err(StatusCode::BAD_REQUEST),
    }

    // Parse channel to find receiver
    let parts: Vec<&str> = channel_id.split(':').collect();
    let receiver_id = parts[2];
    let sender_id = parts[1];

    // Get pools and log acknowledgment
    let pools = state.agent_pools.load();
    let pool = pools
        .values()
        .next()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let logger = ConversationLogger::new(pool.clone());

    let acknowledge_content = format!(
        "[Acknowledgment] Message {} marked as {}",
        request.message_id, request.status
    );

    logger.log_system_message(channel_id, &acknowledge_content);

    // Also log in the sender's channel
    let reverse_channel_id = format!("link:{}:{}", receiver_id, sender_id);
    let reverse_id_str: &str = &reverse_channel_id;
    logger.log_system_message(reverse_id_str, &acknowledge_content);

    let now = chrono::Utc::now().to_rfc3339();

    // Broadcast received event
    let event = crate::api::state::ApiEvent::AgentMessageReceived {
        from_agent_id: sender_id.to_string(),
        to_agent_id: receiver_id.to_string(),
        link_id: format!("{}->{}", sender_id, receiver_id),
        channel_id: channel_id.to_string(),
    };
    state.send_event(event);

    tracing::info!(
        message_id = %request.message_id,
        channel_id = %channel_id,
        status = %request.status,
        "link channel message acknowledged"
    );

    Ok(Json(AcknowledgeMessageResponse {
        success: true,
        message_id: request.message_id,
        acknowledged_at: now,
    }))
}

// --- Get message status ---

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(super) struct LinkMessageStatus {
    pub message_id: String,
    pub channel_id: String,
    pub delivered: bool,
    pub acknowledged: bool,
    pub acknowledged_at: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(super) struct LinkMessageStatusResponse {
    pub status: LinkMessageStatus,
}

/// Get delivery status of a link channel message.
///
/// Checks if the message has been acknowledged by the receiver.
#[utoipa::path(
    get,
    path = "/link-channels/messages/{message_id}/status",
    params(
        ("message_id" = String, Path, description = "Message ID to check"),
    ),
    responses(
        (status = 200, body = LinkMessageStatusResponse),
        (status = 404, description = "Message not found"),
    ),
    tag = "link-channels",
)]
pub(super) async fn get_link_message_status(
    State(state): State<Arc<ApiState>>,
    axum::extract::Path(message_id): axum::extract::Path<String>,
    Query(query): Query<LinkMessagesQuery>,
) -> Result<Json<LinkMessageStatusResponse>, StatusCode> {
    let channel_id = &query.channel_id;

    if !is_valid_link_channel(channel_id) {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Load the message to check if it exists
    let pools = state.agent_pools.load();
    let pool = pools
        .values()
        .next()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let logger = ConversationLogger::new(pool.clone());

    let messages = logger
        .load_channel_transcript(channel_id, 100, None, None, false)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let message = messages
        .iter()
        .find(|m| m.id == message_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    // Check for acknowledgment in the message content
    let acknowledged = message.content.contains("[Acknowledgment]");
    let acknowledged_at = if acknowledged {
        Some(message.created_at.to_rfc3339())
    } else {
        None
    };

    Ok(Json(LinkMessageStatusResponse {
        status: LinkMessageStatus {
            message_id,
            channel_id: channel_id.to_string(),
            delivered: true,
            acknowledged,
            acknowledged_at,
        },
    }))
}

// --- Search link channel messages ---

#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub(super) struct LinkMessageSearchQuery {
    /// Link channel ID to search in
    channel_id: String,
    /// Search text (matched against content)
    q: Option<String>,
    /// Filter by message role
    role: Option<String>,
    /// Filter by sender ID
    sender_id: Option<String>,
    /// Maximum messages to return (default 20, max 100)
    #[serde(default = "default_message_limit")]
    limit: i64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(super) struct LinkMessageSearchResponse {
    pub messages: Vec<LinkChannelMessage>,
    pub total: i64,
}

/// Search messages in a link channel by content, sender, or role.
///
/// Uses LIKE matching for content search (SQLite FTS not available).
#[utoipa::path(
    get,
    path = "/link-channels/messages/search",
    params(
        ("channel_id" = String, Query, description = "Link channel ID to search"),
        ("q" = Option<String>, Query, description = "Search text (matched against content)"),
        ("role" = Option<String>, Query, description = "Filter by role: user, assistant, system"),
        ("sender_id" = Option<String>, Query, description = "Filter by sender ID"),
        ("limit" = i64, Query, description = "Max results (default 20, max 100)"),
    ),
    responses(
        (status = 200, body = LinkMessageSearchResponse),
        (status = 400, description = "Invalid parameters"),
    ),
    tag = "link-channels",
)]
pub(super) async fn search_link_channel_messages(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<LinkMessageSearchQuery>,
) -> Result<Json<LinkMessageSearchResponse>, StatusCode> {
    let channel_id = &query.channel_id;

    if !is_valid_link_channel(channel_id) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let limit = query.limit.min(100).max(1);

    let pools = state.agent_pools.load();
    let pool = pools
        .values()
        .next()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let logger = ConversationLogger::new(pool.clone());

    // Build query based on filters
    let messages = logger
        .load_channel_transcript(channel_id, limit, None, None, false)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Apply in-memory filters
    let filtered: Vec<LinkChannelMessage> = messages
        .into_iter()
        .filter(|msg| {
            if let Some(ref q) = query.q {
                if !msg.content.to_lowercase().contains(&q.to_lowercase()) {
                    return false;
                }
            }
            if let Some(ref role) = query.role {
                if msg.role != *role {
                    return false;
                }
            }
            if let Some(ref sender_id) = query.sender_id {
                if msg.sender_id.as_ref() != Some(sender_id) {
                    return false;
                }
            }
            true
        })
        .map(LinkChannelMessage::from)
        .collect();

    let total = filtered.len() as i64;

    Ok(Json(LinkMessageSearchResponse {
        messages: filtered,
        total,
    }))
}

// --- Message retention (automatic) ---

/// Get information about link channel message persistence.
#[utoipa::path(
    get,
    path = "/link-channels/retention",
    responses(
        (status = 200, body = serde_json::Value),
    ),
    tag = "link-channels",
)]
pub(super) async fn link_channel_retention_info() -> Json<JsonValue> {
    Json(serde_json::json!({
        "persistence": "automatic",
        "storage_table": "conversation_messages",
        "note": "Link channel messages are persisted via ConversationLogger and share the same table as channel messages"
    }))
}

// --- SSE Event subscription for link channels ---

#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub(super) struct LinkChannelSseQuery {
    /// Channel ID to subscribe to (e.g., "link:orchestrator:nexus")
    channel_id: Option<String>,
    /// Agent ID to filter events for (receive messages sent to or received by this agent)
    agent_id: Option<String>,
}

/// SSE endpoint for subscribing to link channel events.
///
/// Allows filtering by channel_id and/or agent_id. When agent_id is provided,
/// receives messages sent to or from that agent.
#[utoipa::path(
    get,
    path = "/link-channels/events",
    params(
        ("channel_id" = Option<String>, Query, description = "Filter by link channel ID"),
        ("agent_id" = Option<String>, Query, description = "Filter by agent ID"),
    ),
    responses(
        (status = 200, description = "SSE event stream", content_type = "text/event-stream"),
    ),
    tag = "link-channels",
)]
pub(super) async fn link_channel_events_sse(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<LinkChannelSseQuery>,
) -> Sse<impl Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
    let mut rx = state.event_tx.subscribe();

    let channel_filter = query.channel_id.map(|c| c.to_lowercase());
    let agent_filter = query.agent_id.clone();

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    // Filter based on query parameters
                    let should_send = match (&event, &channel_filter, &agent_filter) {
                        // AgentMessageSent: filter by channel or agent
                        (ApiEvent::AgentMessageSent { channel_id, from_agent_id, to_agent_id, .. }, channel_id_filter, agent_id_filter) => {
                            let channel_match = channel_id_filter.as_ref()
                                .map(|cf| channel_id.to_lowercase().contains(cf))
                                .unwrap_or(true);
                            let agent_match = agent_id_filter.as_ref()
                                .map(|af| from_agent_id == af || to_agent_id == af)
                                .unwrap_or(true);
                            channel_match && agent_match
                        }
                        // AgentMessageReceived: filter by channel or agent
                        (ApiEvent::AgentMessageReceived { channel_id, from_agent_id, to_agent_id, .. }, channel_id_filter, agent_id_filter) => {
                            let channel_match = channel_id_filter.as_ref()
                                .map(|cf| channel_id.to_lowercase().contains(cf))
                                .unwrap_or(true);
                            let agent_match = agent_id_filter.as_ref()
                                .map(|af| from_agent_id == af || to_agent_id == af)
                                .unwrap_or(true);
                            channel_match && agent_match
                        }
                        // Skip other event types
                        _ => false,
                    };

                    if should_send {
                        if let Ok(json) = serde_json::to_string(&event) {
                            let event_type = match &event {
                                ApiEvent::AgentMessageSent { .. } => "link_message_sent",
                                ApiEvent::AgentMessageReceived { .. } => "link_message_received",
                                _ => "link_event",
                            };
                            yield Ok(axum::response::sse::Event::default()
                                .event(event_type)
                                .data(json));
                        }
                    }
                }
                Err(error) => {
                    match crate::classify_broadcast_recv_result::<ApiEvent>(Err(error)) {
                        crate::BroadcastRecvResult::Lagged(count) => {
                            tracing::debug!(count, "link-channels SSE client lagged");
                            yield Ok(axum::response::sse::Event::default()
                                .event("lagged")
                                .data(format!("{{\"skipped\":{count}}}")));
                        }
                        crate::BroadcastRecvResult::Closed => break,
                        crate::BroadcastRecvResult::Event(_) => unreachable!(),
                    }
                }
            }
        }
    };

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("ping"),
    )
}
