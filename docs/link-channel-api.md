# Link Channel API - Integrated Agent Awareness System

The Link Channel API provides HTTP access to inter-agent communication channels, enabling real-time message delivery, persistence, and notifications between agents.

## Overview

Link channels are internal agent-to-agent communication channels with format `link:agent_a:agent_b`. Each agent pair gets bidirectional channels:

- `link:orchestrator:nexus` - Messages from orchestrator to nexus
- `link:nexus:orchestrator` - Messages from nexus to orchestrator

## Architecture

```
┌─────────┐    HTTP API    ┌──────────────────┐    SSE    ┌─────────┐
│  Agent  │ ──────────────► │ Link Channels  │ ────────► │  Agent  │
│    A    │   POST/GET    │   Service     │  Events   │    B    │
└─────────┘              └──────────────────┘          └─────────┘
       │                      │                      ▲
       │                      ▼
       │              ┌──────────────────┐
       │              │ Conversation  │
       │              │ Logger      │
       │              │ (SQLite)    │
       │              └──────────────────┘
       │                      ▲
       │                      │
       └──────────────────────┘
            Notifications (5 min)
```

## API Endpoints

### Phase 1: Read API

#### List All Link Channels

```bash
GET /api/link-channels
```

Response:
```json
{
  "channels": [
    {
      "channel_id": "link:orchestrator:nexus",
      "from_agent": "orchestrator",
      "to_agent": "nexus",
      "link_kind": "hierarchical",
      "direction": "one_way"
    }
  ]
}
```

#### Read Messages from a Link Channel

```bash
GET /api/link-channels/messages?channel_id=link:orchestrator:nexus&limit=20
```

Query Parameters:
- `channel_id` - Link channel ID (required)
- `limit` - Max messages (default 20, max 100)
- `role` - Filter by role: user, assistant, system
- `sender` - Filter by sender ID or name
- `before` - RFC3339 timestamp (return before)
- `after` - RFC3339 timestamp (return after)
- `oldest_first` - Boolean (default false)

Response:
```json
{
  "messages": [
    {
      "id": "uuid",
      "channel_id": "link:orchestrator:nexus",
      "role": "user",
      "sender_name": "orchestrator",
      "sender_id": "orchestrator",
      "content": "Task update: deployment complete",
      "metadata": "{\"message_type\":\"status_update\"}",
      "created_at": "2026-04-19T10:20:21Z"
    }
  ],
  "has_more": false
}
```

### Phase 2: Write API

#### Send a Message

```bash
POST /api/link-channels/messages
Content-Type: application/json
```

Request:
```json
{
  "channel_id": "link:orchestrator:nexus",
  "content": "Task: Deploy the new feature",
  "message_type": "task_delegation"
}
```

Message Types:
- `text` - Plain text message
- `task_delegation` - Task assignment
- `acknowledgment` - Acknowledgment message
- `status_update` - Status notification

Response:
```json
{
  "success": true,
  "message_id": "uuid",
  "channel_id": "link:orchestrator:nexus",
  "delivered_to": "nexus"
}
```

#### Acknowledge Message Delivery

```bash
POST /api/link-channels/messages/acknowledge
Content-Type: application/json
```

Request:
```json
{
  "channel_id": "link:orchestrator:nexus",
  "message_id": "uuid-of-original-message",
  "status": "delivered"
}
```

Status values: `delivered`, `read`, `processed`

Response:
```json
{
  "success": true,
  "message_id": "uuid-of-original-message",
  "acknowledged_at": "2026-04-19T10:25:00Z"
}
```

#### Get Message Status

```bash
GET /api/link-channels/messages/{message_id}/status?channel_id=link:orchestrator:nexus
```

Response:
```json
{
  "status": {
    "message_id": "uuid",
    "channel_id": "link:orchestrator:nexus",
    "delivered": true,
    "acknowledged": true,
    "acknowledged_at": "2026-04-19T10:25:00Z"
  }
}
```

### Phase 3: Real-Time Events (SSE)

#### Subscribe to Link Channel Events

```bash
GET /api/link-channels/events?agent_id=orchestrator
```

Query Parameters:
- `channel_id` - Filter by channel
- `agent_id` - Filter by agent (receive messages sent to/from this agent)

Response: Server-Sent Events stream

```text
event: link_message_sent
data: {"from_agent_id":"orchestrator","to_agent_id":"nexus","link_id":"orchestrator->nexus","channel_id":"link:orchestrator:nexus"}

event: link_message_received
data: {"from_agent_id":"nexus","to_agent_id":"orchestrator","link_id":"nexus->orchestrator","channel_id":"link:nexus:orchestrator"}
```

Event types:
- `link_message_sent` - Message was sent
- `link_message_received` - Message was acknowledged

### Phase 4: Search & Persistence

#### Search Messages

```bash
GET /api/link-channels/messages/search?channel_id=link:orchestrator:nexus&q=deployment
```

Query Parameters:
- `channel_id` - Required
- `q` - Search text
- `role` - Filter by role
- `sender_id` - Filter by sender
- `limit` - Max results (default 20)

Response:
```json
{
  "messages": [
    {
      "id": "uuid",
      "channel_id": "link:orchestrator:nexus",
      "role": "user",
      "content": "Deploy the new feature",
      "created_at": "2026-04-19T10:20:21Z"
    }
  ],
  "total": 5
}
```

#### Retention Info

```bash
GET /api/link-channels/retention
```

Response:
```json
{
  "persistence": "automatic",
  "storage_table": "conversation_messages",
  "note": "Link channel messages are persisted via ConversationLogger and share the same table as channel messages"
}
```

## Notification System

The link channel notification system runs every 5 minutes in the background:

1. Queries all link channels for unread messages
2. Identifies messages without acknowledgments
3. Sends notifications via working memory

```rust
// Start the notifier
start_link_channel_notifier(pools, links, working_memory).await;
```

Notification behavior:
- Checks for messages without `[Acknowledgment]` prefix
- Limits to 5 most recent messages per check
- Uses working memory to deliver notifications
- Agents receive: "You have N unread message(s) in your link channels"

## Message Flow

### Complete Flow Diagram

```
Phase 1: Read          Phase 2: Write         Phase 3: Events
───────────────        ───────────────        ───────────────
    │                     │                     │
    ▼                     ▼                     ▼
┌─────────┐          ┌─────────┐          ┌──────���──┐
│ Agent A │          │ Agent A │          │  SSE   │
│  sends │────────►│  posts │────────►│ client │
│  HTTP  │         │  API   │         │ stream │
│ request│         │request │         │        │
└────────┘         └────────┘          └────────┘
    │                     │                     │
    ▼                     ▼                     ▼
┌─────────┐          ┌─────────┐          ┌─────────┐
│  DB:    │          │  Both   │          │Real-time│
│ load   │         │ sides   │         │ event  │
│ msgs  │         │logged  │         │emitted │
└────────┘         └────────┘          └────────┘
                           │          
                           ▼          
                    ┌──────────┐        
                    │  Event   │        
                    │  Bus    │        
                    └──────────┘        
```

### Detailed Flow

1. **Send Message**:
   - Client POSTs to `/api/link-channels/messages`
   - API validates channel format and link exists
   - Checks direction (one-way vs two-way)
   - Logs message in both directions via ConversationLogger
   - Broadcasts `AgentMessageSent` event
   - Returns success with message_id

2. **Receive via SSE**:
   - Client connects to `/api/link-channels/events?agent_id=X`
   - Subscribes to event bus
   - When message sent, event emitted with type `link_message_sent`
   - Client receives JSON payload

3. **Acknowledge**:
   - Client POSTs to `/api/link-channels/messages/acknowledge`
   - Logs system message "[Acknowledgment] Message X marked as Y"
   - Broadcasts `AgentMessageReceived` event
   - Message marked as acknowledged

## Usage Examples

### Agent A Notifies Agent B

```bash
# Agent A sends a task delegation
curl -X POST http://localhost:3000/api/link-channels/messages \
  -H "Content-Type: application/json" \
  -d '{
    "channel_id": "link:orchestrator:nexus",
    "content": "Please deploy v2.0 to production",
    "message_type": "task_delegation"
  }'
```

### Agent B Subscribes to Notifications

```bash
# Connect to SSE stream
curl -N http://localhost:3000/api/link-channels/events?agent_id=nexus
# Or query for pending messages
curl "http://localhost:3000/api/link-channels/messages?channel_id=link:nexus:orchestrator&role=user"
```

### Agent B Acknowledges

```bash
curl -X POST http://localhost:3000/api/link-channels/messages/acknowledge \
  -H "Content-Type: application/json" \
  -d '{
    "channel_id": "link:nexus:orchestrator",
    "message_id": "uuid-from-above",
    "status": "read"
  }'
```

### Search History

```bash
curl "http://localhost:3000/api/link-channels/messages/search?channel_id=link:orchestrator:nexus&q=deploy"
```

## Error Handling

| Status | Description |
|--------|-------------|
| 400 | Invalid channel format, missing parameters |
| 401 | Unauthorized |
| 403 | Link direction prevents sending |
| 404 | Link channel not found |
| 500 | Internal server error |

Example error:
```json
{
  "error": "invalid link channel ID format"
}
```

## Integration Points

- **Persistence**: Uses existing `ConversationLogger` → `conversation_messages` table
- **Events**: Uses existing `ApiState::event_tx` broadcast system
- **Links**: Uses existing `AgentLink` configuration
- **Notifications**: Uses `WorkingMemoryStore` for agent notifications

## Files

- `src/api/link_channels.rs` - API handlers
- `src/cron/link_channel_notifications.rs` - Background notification checker
- `tests/link_channels.rs` - Integration tests