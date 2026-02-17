//! Integration tests for request queuing (T023)
//!
//! Tests that requests queue and drain correctly when backends are saturated.

mod common;

use nexus::config::QueueConfig;
use nexus::queue::{Priority, QueueError, RequestQueue};

/// T023: Send N+1 requests to N-capacity queue, verify N accepted, 1 rejected
#[tokio::test]
async fn queue_accepts_up_to_capacity_and_rejects_overflow() {
    let capacity = 5u32;
    let config = QueueConfig {
        enabled: true,
        max_size: capacity,
        max_wait_seconds: 30,
    };
    let queue = RequestQueue::new(config);

    // Enqueue N requests (should all succeed)
    let mut receivers = Vec::new();
    for i in 0..capacity {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let request: nexus::api::ChatCompletionRequest =
            serde_json::from_value(serde_json::json!({
                "model": "test-model",
                "messages": [{"role": "user", "content": format!("msg {}", i)}]
            }))
            .unwrap();

        let intent = nexus::routing::reconciler::intent::RoutingIntent::new(
            format!("req-{}", i),
            "test-model".to_string(),
            "test-model".to_string(),
            nexus::routing::RequestRequirements {
                model: "test-model".to_string(),
                estimated_tokens: 100,
                needs_vision: false,
                needs_tools: false,
                needs_json_mode: false,
                prefers_streaming: false,
            },
            vec![],
        );

        let queued = nexus::queue::QueuedRequest {
            intent,
            request,
            response_tx: tx,
            enqueued_at: std::time::Instant::now(),
            priority: Priority::Normal,
        };

        queue.enqueue(queued).unwrap();
        receivers.push(rx);
    }

    assert_eq!(queue.depth(), capacity as usize);

    // N+1th request should be rejected
    let (tx, _rx) = tokio::sync::oneshot::channel();
    let request: nexus::api::ChatCompletionRequest =
        serde_json::from_value(serde_json::json!({
            "model": "test-model",
            "messages": [{"role": "user", "content": "overflow"}]
        }))
        .unwrap();
    let intent = nexus::routing::reconciler::intent::RoutingIntent::new(
        "req-overflow".to_string(),
        "test-model".to_string(),
        "test-model".to_string(),
        nexus::routing::RequestRequirements {
            model: "test-model".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        },
        vec![],
    );
    let queued = nexus::queue::QueuedRequest {
        intent,
        request,
        response_tx: tx,
        enqueued_at: std::time::Instant::now(),
        priority: Priority::Normal,
    };

    let result = queue.enqueue(queued);
    assert!(matches!(result, Err(QueueError::Full { max_size: 5 })));

    // Dequeue all N, verify they all drain
    for _ in 0..capacity {
        let item = queue.try_dequeue().await;
        assert!(item.is_some());
    }

    assert_eq!(queue.depth(), 0);
}

/// T023: Verify queuing and draining works end-to-end with priority
#[tokio::test]
async fn queue_drains_high_priority_first_integration() {
    let config = QueueConfig {
        enabled: true,
        max_size: 10,
        max_wait_seconds: 30,
    };
    let queue = RequestQueue::new(config);

    // Enqueue: 2 normal, then 1 high, then 1 normal
    let priorities = [
        Priority::Normal,
        Priority::Normal,
        Priority::High,
        Priority::Normal,
    ];

    for (i, &prio) in priorities.iter().enumerate() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let request: nexus::api::ChatCompletionRequest =
            serde_json::from_value(serde_json::json!({
                "model": "test-model",
                "messages": [{"role": "user", "content": format!("msg {}", i)}]
            }))
            .unwrap();
        let intent = nexus::routing::reconciler::intent::RoutingIntent::new(
            format!("req-{}", i),
            "test-model".to_string(),
            "test-model".to_string(),
            nexus::routing::RequestRequirements {
                model: "test-model".to_string(),
                estimated_tokens: 100,
                needs_vision: false,
                needs_tools: false,
                needs_json_mode: false,
                prefers_streaming: false,
            },
            vec![],
        );
        let queued = nexus::queue::QueuedRequest {
            intent,
            request,
            response_tx: tx,
            enqueued_at: std::time::Instant::now(),
            priority: prio,
        };
        queue.enqueue(queued).unwrap();
    }

    assert_eq!(queue.depth(), 4);

    // First dequeue should be high priority
    let first = queue.try_dequeue().await.unwrap();
    assert_eq!(first.priority, Priority::High);

    // Remaining should be normal
    let second = queue.try_dequeue().await.unwrap();
    assert_eq!(second.priority, Priority::Normal);

    let third = queue.try_dequeue().await.unwrap();
    assert_eq!(third.priority, Priority::Normal);

    let fourth = queue.try_dequeue().await.unwrap();
    assert_eq!(fourth.priority, Priority::Normal);

    assert_eq!(queue.depth(), 0);
}
