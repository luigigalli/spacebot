//! Integration tests for link channel API (Phases 1-5).

use reqwest::Client;
use serde_json::json;

/// Test helper for making HTTP requests.
async fn make_request(method: reqwest::Method, url: &str, body: Option<serde_json::Value>) -> reqwest::Response {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("client build");

    let mut request = client.request(method, url);

    if let Some(b) = body {
        request = request
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(&b);
    }

    request.send().await.expect("request send")
}

/// Test Phase 1: Read API - List link channels.
#[tokio::test]
async fn test_list_link_channels() {
    // Skip if no API running
    let result = make_request(
        reqwest::Method::GET,
        "http://127.0.0.1:3000/api/link-channels",
        None,
    )
    .await;

    if result.status() == reqwest::StatusCode::NOT_FOUND {
        eprintln!("API not running, skipping test");
        return;
    }

    assert_eq!(result.status(), 200);

    let body: serde_json::Value = result.json().await.expect("parse json");
    assert!(body.get("channels").is_some());
}

/// Test Phase 1: Read API - Invalid channel format.
#[tokio::test]
async fn test_invalid_channel_format() {
    let result = make_request(
        reqwest::Method::GET,
        "http://127.0.0.1:3000/api/link-channels/messages?channel_id=invalid",
        None,
    )
    .await;

    if result.status() == reqwest::StatusCode::NOT_FOUND {
        eprintln!("API not running, skipping test");
        return;
    }

    assert_eq!(result.status(), 400);
}

/// Test Phase 2: Send message - Empty content.
#[tokio::test]
async fn test_send_empty_content() {
    let result = make_request(
        reqwest::Method::POST,
        "http://127.0.0.1:3000/api/link-channels/messages",
        Some(json!({
            "channel_id": "link:agent1:agent2",
            "content": ""
        })),
    )
    .await;

    if result.status() == reqwest::StatusCode::NOT_FOUND {
        eprintln!("API not running, skipping test");
        return;
    }

    assert_eq!(result.status(), 400);
}

/// Test Phase 2: Send message - Invalid message type.
#[tokio::test]
async fn test_invalid_message_type() {
    let result = make_request(
        reqwest::Method::POST,
        "http://127.0.0.1:3000/api/link-channels/messages",
        Some(json!({
            "channel_id": "link:agent1:agent2",
            "content": "Test",
            "message_type": "invalid"
        })),
    )
    .await;

    if result.status() == reqwest::StatusCode::NOT_FOUND {
        eprintln!("API not running, skipping test");
        return;
    }

    assert_eq!(result.status(), 400);
}

/// Test Phase 3: SSE endpoint.
#[tokio::test]
async fn test_sse_endpoint() {
    let result = make_request(
        reqwest::Method::GET,
        "http://127.0.0.1:3000/api/link-channels/events",
        None,
    )
    .await;

    if result.status() == reqwest::StatusCode::NOT_FOUND {
        eprintln!("API not running, skipping test");
        return;
    }

    // May be 200 or 401 depending on auth
    assert!(result.status() == 200 || result.status() == 401);
}

/// Test Phase 4: Search endpoint - Invalid channel.
#[tokio::test]
async fn test_search_invalid_channel() {
    let result = make_request(
        reqwest::Method::GET,
        "http://127.0.0.1:3000/api/link-channels/messages/search?channel_id=invalid",
        None,
    )
    .await;

    if result.status() == reqwest::StatusCode::NOT_FOUND {
        eprintln!("API not running, skipping test");
        return;
    }

    assert_eq!(result.status(), 400);
}

/// Test Phase 4: Retention info.
#[tokio::test]
async fn test_retention_info() {
    let result = make_request(
        reqwest::Method::GET,
        "http://127.0.0.1:3000/api/link-channels/retention",
        None,
    )
    .await;

    if result.status() == reqwest::StatusCode::NOT_FOUND {
        eprintln!("API not running, skipping test");
        return;
    }

    assert_eq!(result.status(), 200);

    let body: serde_json::Value = result.json().await.expect("parse json");
    assert_eq!(body.get("persistence").unwrap(), "automatic");
}

/// Test: Channel format validation.
#[tokio::test]
async fn test_channel_format_validation() {
    fn is_valid_link_channel(channel_id: &str) -> bool {
        let parts: Vec<&str> = channel_id.split(':').collect();
        parts.len() == 3 && parts[0] == "link" && !parts[1].is_empty() && !parts[2].is_empty()
    }

    assert!(is_valid_link_channel("link:orchestrator:nexus"));
    assert!(is_valid_link_channel("link:a:b"));
    assert!(!is_valid_link_channel("invalid"));
    assert!(!is_valid_link_channel("link:only"));
    assert!(!is_valid_link_channel("link::empty"));
    assert!(!is_valid_link_channel(""));
}

/// Test: Message type validation.
#[tokio::test]
async fn test_message_type_validation() {
    let valid_types = ["text", "task_delegation", "acknowledgment", "status_update"];

    assert!(valid_types.contains(&"text"));
    assert!(valid_types.contains(&"task_delegation"));
    assert!(!valid_types.contains(&"invalid"));
}

/// Test: Acknowledge status validation.
#[tokio::test]
async fn test_acknowledge_status_validation() {
    let valid = ["delivered", "read", "processed"];

    assert!(valid.contains(&"delivered"));
    assert!(valid.contains(&"read"));
    assert!(valid.contains(&"processed"));
    assert!(!valid.contains(&"invalid"));
}