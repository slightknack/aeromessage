# iMessage Database Guide

## Overview

iMessage on macOS stores all messages in a SQLite database. This guide explains the database structure, how to read from it, and important caveats.

## Database Location

```
~/Library/Messages/chat.db
```

The database uses SQLite's WAL (Write-Ahead Logging) mode, so you'll see three files:
- `chat.db` - Main database
- `chat.db-shm` - Shared memory file
- `chat.db-wal` - Write-ahead log

## Access Requirements

You must grant **Full Disk Access** to your terminal/IDE:
1. System Settings → Privacy & Security → Full Disk Access
2. Add Terminal, iTerm, VS Code, or whatever you're using
3. Restart the application

## Core Tables

### `message`
The main table containing all messages.

Key columns:
- `ROWID` - Unique message ID
- `guid` - Globally unique identifier
- `text` - Message text (may be NULL in newer macOS - see attributedBody)
- `attributedBody` - Binary blob containing message text (newer macOS)
- `handle_id` - Foreign key to `handle` table (who sent/received)
- `date` - Timestamp (Apple epoch nanoseconds)
- `date_read` - When message was read
- `is_from_me` - 1 if you sent it, 0 if received
- `is_read` - 1 if read, 0 if unread
- `is_sent` - 1 if sent successfully
- `is_delivered` - 1 if delivered
- `cache_roomnames` - Group chat room name (if applicable)
- `service` - "iMessage", "SMS", or "RCS"
- `associated_message_guid` - For reactions/replies to other messages
- `thread_originator_guid` - For threaded replies

### `handle`
Contact identifiers (phone numbers, emails).

- `ROWID` - Unique ID
- `id` - Phone number or email (e.g., "+14155551234", "john@example.com")
- `service` - "iMessage", "SMS", etc.
- `person_centric_id` - Links to Contacts database

### `chat`
Conversations (1:1 or group).

- `ROWID` - Unique chat ID
- `guid` - Globally unique identifier
- `chat_identifier` - Phone/email for 1:1, UUID for groups
- `display_name` - Group chat name (NULL for 1:1)
- `style` - 43 = group, 45 = 1:1
- `last_read_message_timestamp` - Timestamp of last read message

### `chat_message_join`
Links messages to chats (many-to-many).

- `chat_id` - Foreign key to `chat`
- `message_id` - Foreign key to `message`
- `message_date` - Cached message date for sorting

### `chat_handle_join`
Links chats to participants.

- `chat_id` - Foreign key to `chat`
- `handle_id` - Foreign key to `handle`

## Timestamps

Apple uses a custom epoch: **January 1, 2001 00:00:00 UTC**

In newer macOS (Ventura+), timestamps are in **nanoseconds**. To convert to Unix timestamp:

```python
# Apple epoch offset in seconds
APPLE_EPOCH = 978307200

# If timestamp > 1e12, it's in nanoseconds
def apple_to_unix(apple_ts):
    if apple_ts > 1e12:
        apple_ts = apple_ts / 1e9  # Convert nanoseconds to seconds
    return apple_ts + APPLE_EPOCH
```

## Reading Message Text

### The attributedBody Problem

In macOS Ventura and later, many messages have `text = NULL` and the actual content is in the `attributedBody` blob (a serialized NSAttributedString).

To extract text from attributedBody:

```python
def parse_attributed_body(blob: bytes) -> str:
    """Extract text from attributedBody blob."""
    if not blob:
        return ""
    
    try:
        # Find NSString marker and extract text after it
        text = blob.split(b"NSString")[1]
        text = text[5:]  # Skip 5 bytes after NSString
        
        # Length is either 1 byte or 2 bytes (little-endian)
        length = text[0]
        start = 1
        
        if length == 0x81:  # 129 indicates 2-byte length
            length = int.from_bytes(text[1:3], "little")
            start = 3
        
        return text[start:start + length].decode("utf-8", errors="ignore")
    except (IndexError, ValueError):
        return ""
```

## Example Queries

### Get all unread messages

```sql
SELECT 
    m.ROWID,
    m.text,
    m.attributedBody,
    m.date,
    m.is_from_me,
    h.id as sender,
    c.display_name as group_name,
    c.chat_identifier
FROM message m
JOIN chat_message_join cmj ON m.ROWID = cmj.message_id
JOIN chat c ON cmj.chat_id = c.ROWID
LEFT JOIN handle h ON m.handle_id = h.ROWID
WHERE m.is_read = 0 
  AND m.is_from_me = 0
  AND m.item_type = 0  -- Regular messages only
ORDER BY m.date DESC;
```

### Get conversations with unread counts

```sql
SELECT 
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
GROUP BY c.ROWID
ORDER BY last_message_date DESC;
```

### Get recent messages in a conversation

```sql
SELECT 
    m.ROWID,
    m.text,
    m.attributedBody,
    m.date,
    m.is_from_me,
    h.id as sender
FROM message m
JOIN chat_message_join cmj ON m.ROWID = cmj.message_id
LEFT JOIN handle h ON m.handle_id = h.ROWID
WHERE cmj.chat_id = ?
ORDER BY m.date DESC
LIMIT 20;
```

### Get participants in a group chat

```sql
SELECT h.id, h.service
FROM handle h
JOIN chat_handle_join chj ON h.ROWID = chj.handle_id
WHERE chj.chat_id = ?;
```

## Sending Messages

You cannot write to chat.db directly - the Messages app owns it and changes would be overwritten or cause corruption.

To send messages programmatically, use AppleScript:

```bash
osascript -e 'tell application "Messages" to send "Hello!" to buddy "+14155551234"'
```

For group chats, you need the chat GUID:

```bash
osascript -e 'tell application "Messages" to send "Hello!" to chat id "iMessage;+;chat123456789"'
```

## macOS Contacts Integration

The AddressBook database is at:
```
~/Library/Application Support/AddressBook/AddressBook-v22.abcddb
```

Or use Python with pyobjc:

```python
from Contacts import CNContactStore, CNContactFetchRequest
from Contacts import CNContactGivenNameKey, CNContactFamilyNameKey, CNContactPhoneNumbersKey

store = CNContactStore.alloc().init()
keys = [CNContactGivenNameKey, CNContactFamilyNameKey, CNContactPhoneNumbersKey]
request = CNContactFetchRequest.alloc().initWithKeysToFetch_(keys)

contacts = []
def handler(contact, stop):
    contacts.append(contact)

store.enumerateContactsWithFetchRequest_error_usingBlock_(request, None, handler)
```

## Important Notes

1. **Read-Only Access**: Always open chat.db in read-only mode to avoid conflicts
2. **Copy First**: For safety, consider copying chat.db before querying
3. **WAL Mode**: The database uses WAL, so recent messages may be in chat.db-wal
4. **Attachments**: Stored in `~/Library/Messages/Attachments/`, paths in `attachment` table
5. **Reactions**: Stored as separate messages with `associated_message_type` set

## References

- [imessage_reader](https://github.com/niftycode/imessage_reader) - Python tool for reading messages
- [imessage_tools](https://github.com/my-other-github-account/imessage_tools) - Handles attributedBody parsing
- [pymessage-lite](https://github.com/mattrajca/pymessage-lite) - Simple Python library
- [LangChain iMessage loader](https://api.python.langchain.com/en/latest/_modules/langchain_community/chat_loaders/imessage.html) - Clean attributedBody parsing
