//! Send messages via AppleScript.

use std::process::Command;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SendError {
    #[error("AppleScript failed: {0}")]
    ScriptError(String),
    #[error("Command execution failed: {0}")]
    CommandError(#[from] std::io::Error),
    #[error("Timeout waiting for message send")]
    Timeout,
}

/// Send a message to a chat via Messages.app.
///
/// # Arguments
/// * `chat_identifier` - The chat ID (phone, email, or group ID)
/// * `text` - Message text to send
/// * `is_group` - Whether this is a group chat
///
/// # Returns
/// Ok(()) on success, Err on failure.
pub fn send_message(chat_identifier: &str, text: &str, is_group: bool) -> Result<(), SendError> {
    // Escape quotes and backslashes for AppleScript
    let escaped = text
        .replace('\\', "\\\\")
        .replace('"', "\\\"");

    // Build full chat ID format Messages.app expects
    let full_chat_id = if is_group {
        format!("any;+;{}", chat_identifier)
    } else {
        format!("any;-;{}", chat_identifier)
    };

    let script = format!(
        r#"tell application "Messages"
    set targetChat to chat id "{}"
    send "{}" to targetChat
end tell"#,
        full_chat_id, escaped
    );

    let output = Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(SendError::ScriptError(stderr.to_string()))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_escape_text() {
        // Just test the escaping logic
        let text = r#"Hello "world" \ test"#;
        let escaped = text
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        assert_eq!(escaped, r#"Hello \"world\" \\ test"#);
    }

    // Note: Actual send_message tests would require mocking osascript
    // or running in an environment with Messages.app access.
}
