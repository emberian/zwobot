//! Timer scheduling for rhai-games
//!
//! Provides an event-callback model where scripts can schedule timers
//! that fire and call `on_timer(timer_id, context)` in the script.

use rhai::Dynamic;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::debug;

/// A scheduled timer
#[derive(Clone, Debug)]
pub struct Timer {
    /// Unique timer ID
    pub id: u64,
    /// Topic/session this timer belongs to
    pub topic: String,
    /// Script ID to call back into
    pub script_id: String,
    /// When the timer should fire
    pub fire_at: Instant,
    /// User-provided context data passed to on_timer
    pub context: Dynamic,
    /// If Some, reschedule with this interval after firing
    pub repeating: Option<Duration>,
}

/// Timer manager
pub struct TimerManager {
    timers: RwLock<HashMap<u64, Timer>>,
    next_id: AtomicU64,
}

impl TimerManager {
    /// Create a new timer manager
    pub fn new() -> Self {
        Self {
            timers: RwLock::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }

    /// Schedule a new timer
    ///
    /// Returns the timer ID
    pub async fn schedule(
        &self,
        topic: String,
        script_id: String,
        delay: Duration,
        context: Dynamic,
        repeating: bool,
    ) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let timer = Timer {
            id,
            topic: topic.clone(),
            script_id,
            fire_at: Instant::now() + delay,
            context,
            repeating: if repeating { Some(delay) } else { None },
        };

        self.timers.write().await.insert(id, timer);
        debug!("Scheduled timer {} for topic {} (delay: {:?})", id, topic, delay);
        id
    }

    /// Cancel a timer by ID
    ///
    /// Returns true if the timer was found and cancelled
    pub async fn cancel(&self, timer_id: u64) -> bool {
        let removed = self.timers.write().await.remove(&timer_id).is_some();
        if removed {
            debug!("Cancelled timer {}", timer_id);
        }
        removed
    }

    /// Collect all timers ready to fire (past their fire_at time)
    ///
    /// Removes one-shot timers from the manager.
    /// Repeating timers are rescheduled automatically.
    pub async fn collect_ready(&self) -> Vec<Timer> {
        let now = Instant::now();
        let mut timers = self.timers.write().await;
        let mut ready = Vec::new();
        let mut to_remove = Vec::new();
        let mut to_reschedule = Vec::new();

        for (id, timer) in timers.iter() {
            if timer.fire_at <= now {
                ready.push(timer.clone());
                if timer.repeating.is_some() {
                    to_reschedule.push(*id);
                } else {
                    to_remove.push(*id);
                }
            }
        }

        // Remove one-shot timers
        for id in to_remove {
            timers.remove(&id);
        }

        // Reschedule repeating timers
        for id in to_reschedule {
            if let Some(timer) = timers.get_mut(&id) {
                let interval = timer.repeating.unwrap();
                timer.fire_at = now + interval;
                debug!("Rescheduled repeating timer {} for {:?}", id, interval);
            }
        }

        ready
    }

    /// Cancel all timers for a topic
    ///
    /// Called when a session ends
    pub async fn cancel_all_for_topic(&self, topic: &str) {
        let mut timers = self.timers.write().await;
        let to_remove: Vec<u64> = timers
            .iter()
            .filter(|(_, t)| t.topic == topic)
            .map(|(id, _)| *id)
            .collect();

        for id in &to_remove {
            timers.remove(id);
        }

        if !to_remove.is_empty() {
            debug!("Cancelled {} timers for topic {}", to_remove.len(), topic);
        }
    }

    /// Get timer IDs for a topic
    pub async fn timers_for_topic(&self, topic: &str) -> Vec<u64> {
        self.timers
            .read()
            .await
            .iter()
            .filter(|(_, t)| t.topic == topic)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Get the number of active timers
    pub async fn count(&self) -> usize {
        self.timers.read().await.len()
    }
}

impl Default for TimerManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_schedule_and_cancel() {
        let manager = TimerManager::new();

        let id = manager
            .schedule(
                "topic1".to_string(),
                "script1".to_string(),
                Duration::from_secs(60),
                Dynamic::from("test"),
                false,
            )
            .await;

        assert_eq!(manager.count().await, 1);
        assert!(manager.cancel(id).await);
        assert_eq!(manager.count().await, 0);
        assert!(!manager.cancel(id).await); // Already cancelled
    }

    #[tokio::test]
    async fn test_collect_ready() {
        let manager = TimerManager::new();

        // Schedule a timer that fires immediately
        manager
            .schedule(
                "topic1".to_string(),
                "script1".to_string(),
                Duration::from_millis(0),
                Dynamic::from("context"),
                false,
            )
            .await;

        // Wait a bit
        tokio::time::sleep(Duration::from_millis(10)).await;

        let ready = manager.collect_ready().await;
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].topic, "topic1");

        // Timer should be removed (one-shot)
        assert_eq!(manager.count().await, 0);
    }

    #[tokio::test]
    async fn test_repeating_timer() {
        let manager = TimerManager::new();

        // Schedule a repeating timer
        let id = manager
            .schedule(
                "topic1".to_string(),
                "script1".to_string(),
                Duration::from_millis(0),
                Dynamic::from("context"),
                true, // repeating
            )
            .await;

        tokio::time::sleep(Duration::from_millis(10)).await;

        let ready = manager.collect_ready().await;
        assert_eq!(ready.len(), 1);

        // Timer should still exist (repeating)
        assert_eq!(manager.count().await, 1);

        // Clean up
        manager.cancel(id).await;
    }

    #[tokio::test]
    async fn test_cancel_all_for_topic() {
        let manager = TimerManager::new();

        manager
            .schedule(
                "topic1".to_string(),
                "script1".to_string(),
                Duration::from_secs(60),
                Dynamic::UNIT,
                false,
            )
            .await;
        manager
            .schedule(
                "topic1".to_string(),
                "script1".to_string(),
                Duration::from_secs(60),
                Dynamic::UNIT,
                false,
            )
            .await;
        manager
            .schedule(
                "topic2".to_string(),
                "script1".to_string(),
                Duration::from_secs(60),
                Dynamic::UNIT,
                false,
            )
            .await;

        assert_eq!(manager.count().await, 3);

        manager.cancel_all_for_topic("topic1").await;

        assert_eq!(manager.count().await, 1);
        assert_eq!(manager.timers_for_topic("topic2").await.len(), 1);
    }
}
