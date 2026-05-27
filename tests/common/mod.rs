//! 共享测试工具

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::sleep;

use lumino_collaboration::client::CollaborationEvent;

/// 等待事件的辅助结构
#[derive(Clone)]
pub struct EventCollector {
    events: Arc<Mutex<Vec<CollaborationEvent>>>,
}

impl EventCollector {
    pub fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn callback(&self) -> impl Fn(CollaborationEvent) + Clone + Send + 'static {
        let events = self.events.clone();
        move |event| {
            let events = events.clone();
            let event_clone = event.clone();
            tokio::spawn(async move {
                let mut lock = events.lock().await;
                println!("  [Event] 收到事件: {:?}", event_clone);
                lock.push(event_clone);
            });
        }
    }

    pub async fn wait_for<T, F>(&self, predicate: F, timeout_ms: u64) -> Option<T>
    where
        F: Fn(&CollaborationEvent) -> Option<T>,
    {
        let start = std::time::Instant::now();
        while start.elapsed().as_millis() < timeout_ms as u128 {
            {
                let lock = self.events.lock().await;
                for event in lock.iter().rev() {
                    if let Some(result) = predicate(event) {
                        return Some(result);
                    }
                }
            }
            sleep(Duration::from_millis(100)).await;
        }
        None
    }

    pub async fn contains_event<F>(&self, predicate: F, timeout_ms: u64) -> bool
    where
        F: Fn(&CollaborationEvent) -> bool,
    {
        self.wait_for(|e| if predicate(e) { Some(()) } else { None }, timeout_ms)
            .await
            .is_some()
    }

    pub async fn get_events(&self) -> Vec<CollaborationEvent> {
        self.events.lock().await.clone()
    }
}
