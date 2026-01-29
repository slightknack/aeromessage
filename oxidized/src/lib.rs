//! Aeromessage - Batch-reply to iMessages
//!
//! Core library for reading iMessage database and sending messages.

mod db;
mod models;
mod contacts;
mod send;

pub use db::{Database, mark_as_read};
pub use models::{Conversation, Message, Attachment, Reaction};
pub use contacts::ContactResolver;
pub use send::send_message;

/// Apple epoch: January 1, 2001 00:00:00 UTC
pub const APPLE_EPOCH_OFFSET: i64 = 978307200;

/// Convert Apple timestamp to Unix timestamp.
/// Apple timestamps may be in seconds or nanoseconds.
pub fn apple_to_unix(apple_ts: i64) -> i64 {
    let ts = if apple_ts > 1_000_000_000_000 {
        apple_ts / 1_000_000_000 // nanoseconds to seconds
    } else {
        apple_ts
    };
    ts + APPLE_EPOCH_OFFSET
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apple_to_unix_seconds() {
        // 2024-01-01 00:00:00 in Apple time = 725846400
        let apple_ts = 725846400_i64;
        let unix_ts = apple_to_unix(apple_ts);
        // Should be 2024-01-01 00:00:00 UTC = 1704067200
        assert_eq!(unix_ts, 1704153600);
    }

    #[test]
    fn test_apple_to_unix_nanoseconds() {
        // Same time but in nanoseconds
        let apple_ts = 725846400_000_000_000_i64;
        let unix_ts = apple_to_unix(apple_ts);
        assert_eq!(unix_ts, 1704153600);
    }
}
