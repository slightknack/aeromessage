//! Data models for iMessage conversations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Reaction emoji mappings by associated_message_type.
pub const REACTION_EMOJI: &[(i32, &str)] = &[
    (2000, "❤️"),  // Loved
    (2001, "👍"),  // Liked
    (2002, "👎"),  // Disliked
    (2003, "😂"),  // Laughed
    (2004, "‼️"),  // Emphasized
    (2005, "❓"),  // Questioned
    (2006, "🫶"),  // Heart hands
];

/// Get emoji for a reaction type code.
pub fn reaction_emoji(code: i32) -> Option<&'static str> {
    REACTION_EMOJI
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, e)| *e)
}

/// A message attachment (image, file, etc).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub filename: String,
    pub mime_type: String,
    pub transfer_name: String,
}

impl Attachment {
    /// Check if this attachment is an image.
    pub fn is_image(&self) -> bool {
        self.mime_type.starts_with("image/")
    }

    /// Get the URL path for serving this attachment.
    /// Returns None if filename doesn't have expected prefix.
    pub fn url_path(&self) -> Option<String> {
        const PREFIX: &str = "~/Library/Messages/Attachments/";
        if self.filename.starts_with(PREFIX) {
            Some(format!("/attachment/{}", &self.filename[PREFIX.len()..]))
        } else {
            None
        }
    }
}

/// A reaction on a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reaction {
    pub emoji: String,
    pub is_from_me: bool,
    pub sender: Option<String>,
}

/// A single message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub rowid: i64,
    pub guid: String,
    pub text: String,
    pub date: DateTime<Utc>,
    pub is_from_me: bool,
    pub sender: Option<String>,
    pub attachments: Vec<Attachment>,
    pub reactions: Vec<Reaction>,
}

impl Message {
    /// Get display text with attachment placeholders removed.
    pub fn display_text(&self) -> String {
        self.text.replace('\u{FFFC}', "").trim().to_string()
    }

    /// Check if this message is image-only (has images but no text).
    pub fn is_image_only(&self) -> bool {
        let has_image = self.attachments.iter().any(|a| a.is_image());
        has_image && self.display_text().is_empty()
    }

    /// Get unique reaction emojis as a combined string.
    pub fn reaction_summary(&self) -> String {
        let mut seen = Vec::new();
        for r in &self.reactions {
            if !seen.contains(&r.emoji) {
                seen.push(r.emoji.clone());
            }
        }
        seen.join("")
    }
}

/// A conversation with messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub chat_id: i64,
    pub display_name: Option<String>,
    pub chat_identifier: String,
    pub style: i32, // 43 = group, 45 = 1:1
    pub unread_count: i64,
    pub last_message_date: DateTime<Utc>,
    pub messages: Vec<Message>,
    pub participants: Vec<String>,
    /// Resolved name (from contacts or people.tsv)
    pub resolved_name: Option<String>,
}

impl Conversation {
    /// Check if this is a group conversation.
    pub fn is_group(&self) -> bool {
        self.style == 43
    }

    /// Get the best display name for this conversation.
    pub fn name(&self) -> &str {
        if let Some(ref name) = self.display_name {
            if !name.is_empty() {
                return name;
            }
        }
        if let Some(ref name) = self.resolved_name {
            return name;
        }
        &self.chat_identifier
    }

    /// Get URL to open this conversation in Messages.app.
    pub fn messages_url(&self) -> String {
        if self.is_group() {
            format!("imessage://?groupID={}", self.chat_identifier)
        } else {
            format!("imessage://{}", self.chat_identifier)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reaction_emoji_lookup() {
        assert_eq!(reaction_emoji(2000), Some("❤️"));
        assert_eq!(reaction_emoji(2001), Some("👍"));
        assert_eq!(reaction_emoji(9999), None);
    }

    #[test]
    fn test_attachment_is_image() {
        let img = Attachment {
            filename: "test.jpg".into(),
            mime_type: "image/jpeg".into(),
            transfer_name: "test.jpg".into(),
        };
        assert!(img.is_image());

        let pdf = Attachment {
            filename: "doc.pdf".into(),
            mime_type: "application/pdf".into(),
            transfer_name: "doc.pdf".into(),
        };
        assert!(!pdf.is_image());
    }

    #[test]
    fn test_attachment_url_path() {
        let att = Attachment {
            filename: "~/Library/Messages/Attachments/ab/cd/file.jpg".into(),
            mime_type: "image/jpeg".into(),
            transfer_name: "file.jpg".into(),
        };
        assert_eq!(att.url_path(), Some("/attachment/ab/cd/file.jpg".into()));

        let other = Attachment {
            filename: "/some/other/path.jpg".into(),
            mime_type: "image/jpeg".into(),
            transfer_name: "path.jpg".into(),
        };
        assert_eq!(other.url_path(), None);
    }

    #[test]
    fn test_message_display_text() {
        let msg = Message {
            rowid: 1,
            guid: "test".into(),
            text: "Hello \u{FFFC} world".into(),
            date: Utc::now(),
            is_from_me: false,
            sender: None,
            attachments: vec![],
            reactions: vec![],
        };
        assert_eq!(msg.display_text(), "Hello  world");
    }

    #[test]
    fn test_conversation_is_group() {
        let group = Conversation {
            chat_id: 1,
            display_name: None,
            chat_identifier: "chat123".into(),
            style: 43,
            unread_count: 5,
            last_message_date: Utc::now(),
            messages: vec![],
            participants: vec![],
            resolved_name: None,
        };
        assert!(group.is_group());

        let direct = Conversation {
            style: 45,
            ..group.clone()
        };
        assert!(!direct.is_group());
    }

    #[test]
    fn test_conversation_name_priority() {
        // display_name takes priority
        let conv = Conversation {
            chat_id: 1,
            display_name: Some("Group Chat".into()),
            chat_identifier: "+15551234567".into(),
            style: 45,
            unread_count: 1,
            last_message_date: Utc::now(),
            messages: vec![],
            participants: vec![],
            resolved_name: Some("John Doe".into()),
        };
        assert_eq!(conv.name(), "Group Chat");

        // resolved_name if no display_name
        let conv2 = Conversation {
            display_name: None,
            ..conv.clone()
        };
        assert_eq!(conv2.name(), "John Doe");

        // chat_identifier as fallback
        let conv3 = Conversation {
            display_name: None,
            resolved_name: None,
            ..conv
        };
        assert_eq!(conv3.name(), "+15551234567");
    }

    #[test]
    fn test_conversation_messages_url() {
        let direct = Conversation {
            chat_id: 1,
            display_name: None,
            chat_identifier: "+15551234567".into(),
            style: 45,
            unread_count: 1,
            last_message_date: Utc::now(),
            messages: vec![],
            participants: vec![],
            resolved_name: None,
        };
        assert_eq!(direct.messages_url(), "imessage://+15551234567");

        let group = Conversation {
            style: 43,
            chat_identifier: "chat123456".into(),
            ..direct
        };
        assert_eq!(group.messages_url(), "imessage://?groupID=chat123456");
    }

    #[test]
    fn test_message_is_image_only() {
        let img_attachment = Attachment {
            filename: "photo.jpg".into(),
            mime_type: "image/jpeg".into(),
            transfer_name: "photo.jpg".into(),
        };

        // Image with no text
        let msg = Message {
            rowid: 1,
            guid: "test".into(),
            text: "\u{FFFC}".into(), // Just placeholder
            date: Utc::now(),
            is_from_me: false,
            sender: None,
            attachments: vec![img_attachment.clone()],
            reactions: vec![],
        };
        assert!(msg.is_image_only());

        // Image with text
        let msg_with_text = Message {
            text: "Check this out \u{FFFC}".into(),
            ..msg.clone()
        };
        assert!(!msg_with_text.is_image_only());

        // No attachments
        let msg_no_att = Message {
            attachments: vec![],
            ..msg
        };
        assert!(!msg_no_att.is_image_only());
    }

    #[test]
    fn test_message_reaction_summary() {
        let msg = Message {
            rowid: 1,
            guid: "test".into(),
            text: "Hello".into(),
            date: Utc::now(),
            is_from_me: false,
            sender: None,
            attachments: vec![],
            reactions: vec![
                Reaction { emoji: "❤️".into(), is_from_me: false, sender: None },
                Reaction { emoji: "👍".into(), is_from_me: true, sender: None },
                Reaction { emoji: "❤️".into(), is_from_me: true, sender: None }, // Duplicate
            ],
        };
        assert_eq!(msg.reaction_summary(), "❤️👍");
    }

    #[test]
    fn test_conversation_empty_display_name() {
        let conv = Conversation {
            chat_id: 1,
            display_name: Some("".into()), // Empty string
            chat_identifier: "+15551234567".into(),
            style: 45,
            unread_count: 1,
            last_message_date: Utc::now(),
            messages: vec![],
            participants: vec![],
            resolved_name: Some("John".into()),
        };
        // Should skip empty display_name and use resolved_name
        assert_eq!(conv.name(), "John");
    }

    #[test]
    fn test_all_reaction_types() {
        assert_eq!(reaction_emoji(2000), Some("❤️"));
        assert_eq!(reaction_emoji(2001), Some("👍"));
        assert_eq!(reaction_emoji(2002), Some("👎"));
        assert_eq!(reaction_emoji(2003), Some("😂"));
        assert_eq!(reaction_emoji(2004), Some("‼️"));
        assert_eq!(reaction_emoji(2005), Some("❓"));
        assert_eq!(reaction_emoji(2006), Some("🫶"));
    }
}
