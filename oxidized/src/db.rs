//! iMessage database access.

use std::path::PathBuf;
use rusqlite::{Connection, OpenFlags};
use thiserror::Error;

use crate::models::{Conversation, Message, Attachment, Reaction, reaction_emoji};
use crate::apple_to_unix;
use chrono::{DateTime, Utc};

#[derive(Error, Debug)]
pub enum DbError {
    #[error("Database not found at {0}")]
    NotFound(PathBuf),
    #[error("Permission denied: {0}")]
    PermissionDenied(PathBuf),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// Handle to the iMessage database.
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Default chat.db path.
    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .expect("home directory required")
            .join("Library/Messages/chat.db")
    }

    /// Open the database read-only.
    pub fn open(path: &PathBuf) -> Result<Self, DbError> {
        if !path.exists() {
            return Err(DbError::NotFound(path.clone()));
        }

        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ).map_err(|e| {
            if e.to_string().contains("unable to open") {
                DbError::PermissionDenied(path.clone())
            } else {
                DbError::Sqlite(e)
            }
        })?;

        Ok(Self { conn })
    }

    /// Get all conversations with unread messages.
    pub fn unread_conversations(&self) -> Result<Vec<Conversation>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT 
                c.ROWID as chat_id,
                c.display_name,
                c.chat_identifier,
                c.style,
                COUNT(*) as unread_count,
                MAX(m.date) as last_message_date
            FROM chat c
            JOIN chat_message_join cmj ON c.ROWID = cmj.chat_id
            JOIN message m ON cmj.message_id = m.ROWID
            WHERE m.is_read = 0 
              AND m.is_from_me = 0
              AND m.item_type = 0
              AND m.is_finished = 1
              AND c.is_filtered != 2
            GROUP BY c.ROWID
            ORDER BY last_message_date DESC"
        )?;

        let mut conversations = Vec::new();
        let rows = stmt.query_map([], |row| {
            let apple_ts: i64 = row.get(5)?;
            let unix_ts = apple_to_unix(apple_ts);
            let date = DateTime::from_timestamp(unix_ts, 0)
                .unwrap_or_else(Utc::now);

            Ok(Conversation {
                chat_id: row.get(0)?,
                display_name: row.get(1)?,
                chat_identifier: row.get(2)?,
                style: row.get(3)?,
                unread_count: row.get(4)?,
                last_message_date: date,
                messages: Vec::new(),
                participants: Vec::new(),
                resolved_name: None,
            })
        })?;

        for row in rows {
            conversations.push(row?);
        }

        // Load participants and messages for each conversation
        for conv in &mut conversations {
            self.load_participants(conv)?;
            self.load_messages(conv)?;
        }

        Ok(conversations)
    }

    fn load_participants(&self, conv: &mut Conversation) -> Result<(), DbError> {
        if !conv.is_group() {
            return Ok(());
        }

        let mut stmt = self.conn.prepare(
            "SELECT h.id FROM handle h
             JOIN chat_handle_join chj ON h.ROWID = chj.handle_id
             WHERE chj.chat_id = ?"
        )?;

        let rows = stmt.query_map([conv.chat_id], |row| row.get(0))?;
        for row in rows {
            conv.participants.push(row?);
        }

        Ok(())
    }

    fn load_messages(&self, conv: &mut Conversation) -> Result<(), DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT 
                m.ROWID,
                m.guid,
                m.text,
                m.attributedBody,
                m.date,
                m.is_from_me,
                m.cache_has_attachments,
                h.id as sender
            FROM message m
            JOIN chat_message_join cmj ON m.ROWID = cmj.message_id
            LEFT JOIN handle h ON m.handle_id = h.ROWID
            WHERE cmj.chat_id = ?
              AND m.item_type = 0
              AND m.associated_message_type = 0
            ORDER BY m.date DESC
            LIMIT 15"
        )?;

        let mut messages = Vec::new();
        let mut guids = Vec::new();

        let rows = stmt.query_map([conv.chat_id], |row| {
            let rowid: i64 = row.get(0)?;
            let guid: String = row.get(1)?;
            let text: Option<String> = row.get(2)?;
            let attributed_body: Option<Vec<u8>> = row.get(3)?;
            let apple_ts: i64 = row.get(4)?;
            let is_from_me: bool = row.get(5)?;
            let has_attachments: bool = row.get(6)?;
            let sender: Option<String> = row.get(7)?;

            Ok((rowid, guid, text, attributed_body, apple_ts, is_from_me, has_attachments, sender))
        })?;

        for row in rows {
            let (rowid, guid, text, attributed_body, apple_ts, is_from_me, has_attachments, sender) = row?;

            // Try text first, then parse attributedBody
            let final_text = text
                .filter(|t| !t.is_empty())
                .or_else(|| attributed_body.and_then(|b| parse_attributed_body(&b)))
                .unwrap_or_default();

            let unix_ts = apple_to_unix(apple_ts);
            let date = DateTime::from_timestamp(unix_ts, 0).unwrap_or_else(Utc::now);

            // Load attachments if present
            let attachments = if has_attachments {
                self.load_attachments(rowid)?
            } else {
                Vec::new()
            };

            // Only include if has text or attachments
            if !final_text.trim().is_empty() || !attachments.is_empty() {
                guids.push(guid.clone());
                messages.push(Message {
                    rowid,
                    guid,
                    text: final_text,
                    date,
                    is_from_me,
                    sender,
                    attachments,
                    reactions: Vec::new(),
                });
            }
        }

        // Load reactions
        if !guids.is_empty() {
            self.load_reactions(&mut messages, &guids)?;
        }

        // Reverse to chronological order
        messages.reverse();
        conv.messages = messages;

        Ok(())
    }

    fn load_attachments(&self, message_rowid: i64) -> Result<Vec<Attachment>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT a.filename, a.mime_type, a.transfer_name
             FROM attachment a
             JOIN message_attachment_join maj ON a.ROWID = maj.attachment_id
             WHERE maj.message_id = ?"
        )?;

        let mut attachments = Vec::new();
        let rows = stmt.query_map([message_rowid], |row| {
            Ok(Attachment {
                filename: row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                mime_type: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                transfer_name: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            })
        })?;

        for row in rows {
            let att = row?;
            if !att.filename.is_empty() {
                attachments.push(att);
            }
        }

        Ok(attachments)
    }

    fn load_reactions(&self, messages: &mut [Message], guids: &[String]) -> Result<(), DbError> {
        // Build prefixed GUIDs for lookup
        let mut prefixed: Vec<String> = Vec::with_capacity(guids.len() * 3);
        for guid in guids {
            prefixed.push(format!("p:0/{}", guid));
            prefixed.push(format!("p:1/{}", guid));
            prefixed.push(format!("bp:{}", guid));
        }

        if prefixed.is_empty() {
            return Ok(());
        }

        // Build query with placeholders
        let placeholders: String = prefixed.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "SELECT m.associated_message_guid, m.associated_message_type, m.is_from_me, h.id
             FROM message m
             LEFT JOIN handle h ON m.handle_id = h.ROWID
             WHERE m.associated_message_guid IN ({})
               AND m.associated_message_type IN (2000, 2001, 2002, 2003, 2004, 2005, 2006)",
            placeholders
        );

        let mut stmt = self.conn.prepare(&query)?;
        let params: Vec<&dyn rusqlite::ToSql> = prefixed.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        let rows = stmt.query_map(params.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i32>(1)?,
                row.get::<_, bool>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?;

        // Build guid -> message index map
        let guid_map: std::collections::HashMap<String, usize> = messages
            .iter()
            .enumerate()
            .map(|(i, m)| (m.guid.clone(), i))
            .collect();

        for row in rows {
            let (assoc_guid, reaction_type, is_from_me, sender) = row?;

            // Extract target GUID from "p:0/GUID" or "bp:GUID" format
            let target_guid = if assoc_guid.starts_with("p:") {
                assoc_guid.split('/').nth(1).map(|s| s.to_string())
            } else if assoc_guid.starts_with("bp:") {
                Some(assoc_guid[3..].to_string())
            } else {
                Some(assoc_guid)
            };

            if let Some(target) = target_guid {
                if let Some(&idx) = guid_map.get(&target) {
                    if let Some(emoji) = reaction_emoji(reaction_type) {
                        messages[idx].reactions.push(Reaction {
                            emoji: emoji.to_string(),
                            is_from_me,
                            sender,
                        });
                    }
                }
            }
        }

        Ok(())
    }
}

/// Mark all messages in a chat as read.
/// This opens a separate write connection since the main Database is read-only.
pub fn mark_as_read(chat_identifier: &str) -> Result<usize, DbError> {
    let path = Database::default_path();
    let conn = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    
    let affected = conn.execute(
        "UPDATE message SET is_read = 1
         WHERE ROWID IN (
             SELECT m.ROWID FROM message m
             JOIN chat_message_join cmj ON m.ROWID = cmj.message_id
             JOIN chat c ON cmj.chat_id = c.ROWID
             WHERE c.chat_identifier = ? AND m.is_read = 0
         )",
        [chat_identifier],
    )?;
    
    Ok(affected)
}

/// Parse text from attributedBody blob.
fn parse_attributed_body(blob: &[u8]) -> Option<String> {
    // Find NSString marker
    let marker = b"NSString";
    let pos = blob.windows(marker.len()).position(|w| w == marker)?;
    let after = &blob[pos + marker.len()..];

    if after.len() < 6 {
        return None;
    }

    // Skip 5 bytes after NSString
    let data = &after[5..];
    if data.is_empty() {
        return None;
    }

    // Length is 1 byte, or if 0x81, next 2 bytes (little-endian)
    let (length, start) = if data[0] == 0x81 && data.len() >= 3 {
        let len = u16::from_le_bytes([data[1], data[2]]) as usize;
        (len, 3)
    } else {
        (data[0] as usize, 1)
    };

    if start + length > data.len() {
        return None;
    }

    String::from_utf8(data[start..start + length].to_vec()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_attributed_body_simple() {
        // Minimal NSString blob: marker + 5 bytes + 1 byte length + text
        let mut blob = Vec::new();
        blob.extend_from_slice(b"prefix");
        blob.extend_from_slice(b"NSString");
        blob.extend_from_slice(&[0, 0, 0, 0, 0]); // 5 bytes padding
        blob.push(5); // length
        blob.extend_from_slice(b"Hello");

        assert_eq!(parse_attributed_body(&blob), Some("Hello".to_string()));
    }

    #[test]
    fn test_parse_attributed_body_empty() {
        assert_eq!(parse_attributed_body(&[]), None);
        assert_eq!(parse_attributed_body(b"no marker here"), None);
    }

    #[test]
    fn test_default_path() {
        let path = Database::default_path();
        assert!(path.to_string_lossy().contains("Library/Messages/chat.db"));
    }

    #[test]
    fn test_parse_attributed_body_long_length() {
        // Test 0x81 prefix for longer strings (>127 bytes)
        let mut blob = Vec::new();
        blob.extend_from_slice(b"NSString");
        blob.extend_from_slice(&[0, 0, 0, 0, 0]); // 5 bytes padding
        blob.push(0x81); // Long length marker
        blob.extend_from_slice(&[10, 0]); // 10 in little-endian
        blob.extend_from_slice(b"0123456789");

        assert_eq!(parse_attributed_body(&blob), Some("0123456789".to_string()));
    }

    #[test]
    fn test_parse_attributed_body_truncated() {
        // NSString marker but not enough data after
        let blob = b"NSString12345";
        assert_eq!(parse_attributed_body(blob), None);
    }

    #[test]
    fn test_parse_attributed_body_length_exceeds_data() {
        let mut blob = Vec::new();
        blob.extend_from_slice(b"NSString");
        blob.extend_from_slice(&[0, 0, 0, 0, 0]);
        blob.push(100); // Length says 100 bytes
        blob.extend_from_slice(b"short"); // Only 5 bytes

        assert_eq!(parse_attributed_body(&blob), None);
    }

    #[test]
    fn test_db_error_display() {
        let err = DbError::NotFound(PathBuf::from("/test/path"));
        assert!(err.to_string().contains("/test/path"));

        let err = DbError::PermissionDenied(PathBuf::from("/secret"));
        assert!(err.to_string().contains("Permission denied"));
    }
}
