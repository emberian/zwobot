//! Zulip tracing subscriber for logging to a Zulip topic.

use crate::zulip::ZulipClient;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;

/// A tracing layer that sends log messages to a Zulip topic.
pub struct ZulipLayer {
    /// Channel to send log messages for batching
    sender: mpsc::UnboundedSender<LogMessage>,
    /// Minimum level to log to Zulip
    min_level: Level,
}

struct LogMessage {
    level: Level,
    message: String,
    target: String,
}

impl ZulipLayer {
    /// Create a new ZulipLayer that logs to the specified channel/topic.
    ///
    /// Returns the layer and a task handle that must be spawned.
    pub fn new(
        zulip: Arc<ZulipClient>,
        channel: String,
        topic: String,
        min_level: Level,
    ) -> (Self, tokio::task::JoinHandle<()>) {
        let (sender, receiver) = mpsc::unbounded_channel();

        let task = tokio::spawn(log_worker(zulip, channel, topic, receiver));

        (Self { sender, min_level }, task)
    }
}

impl<S> Layer<S> for ZulipLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        let level = *metadata.level();

        // Only log events at or above the minimum level
        if level > self.min_level {
            return;
        }

        // Skip zulip module logs to avoid infinite loops
        let target = metadata.target();
        if target.starts_with("zwobot::zulip") {
            return;
        }

        // Extract message from event
        let mut visitor = MessageVisitor::new();
        event.record(&mut visitor);

        if let Some(message) = visitor.message {
            let _ = self.sender.send(LogMessage {
                level,
                message,
                target: target.to_string(),
            });
        }
    }
}

/// Visitor to extract the message field from a tracing event
struct MessageVisitor {
    message: Option<String>,
}

impl MessageVisitor {
    fn new() -> Self {
        Self { message: None }
    }
}

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{:?}", value));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        }
    }
}

/// Worker task that batches and sends log messages to Zulip
async fn log_worker(
    zulip: Arc<ZulipClient>,
    channel: String,
    topic: String,
    mut receiver: mpsc::UnboundedReceiver<LogMessage>,
) {
    let mut buffer = Vec::new();
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));

    loop {
        tokio::select! {
            msg = receiver.recv() => {
                match msg {
                    Some(msg) => {
                        buffer.push(msg);
                        // Send immediately if buffer is large
                        if buffer.len() >= 20 {
                            send_batch(&zulip, &channel, &topic, &mut buffer).await;
                        }
                    }
                    None => break, // Channel closed
                }
            }
            _ = interval.tick() => {
                // Send buffered messages every 5 seconds
                if !buffer.is_empty() {
                    send_batch(&zulip, &channel, &topic, &mut buffer).await;
                }
            }
        }
    }

    // Send any remaining messages before exiting
    if !buffer.is_empty() {
        send_batch(&zulip, &channel, &topic, &mut buffer).await;
    }
}

async fn send_batch(
    zulip: &ZulipClient,
    channel: &str,
    topic: &str,
    buffer: &mut Vec<LogMessage>,
) {
    if buffer.is_empty() {
        return;
    }

    let mut message = String::from("```\n");

    for log in buffer.drain(..) {
        let level_str = match log.level {
            Level::ERROR => "ERROR",
            Level::WARN => "WARN ",
            Level::INFO => "INFO ",
            Level::DEBUG => "DEBUG",
            Level::TRACE => "TRACE",
        };

        message.push_str(&format!("[{}] {}: {}\n", level_str, log.target, log.message));
    }

    message.push_str("```");

    // Best effort - don't propagate errors
    let _ = zulip.send_message(channel, topic, &message).await;
}
