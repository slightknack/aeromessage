#!/usr/bin/env python3
"""
iMessage Inbox Crusher - Batch process your unread messages
"""

import csv
import os
import re
import sqlite3
import subprocess
import sys
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import Optional

from flask import Flask, render_template, request, jsonify, send_file, abort
from markupsafe import Markup, escape

# Apple epoch: January 1, 2001
APPLE_EPOCH = 978307200

# Database path
CHAT_DB = Path.home() / "Library" / "Messages" / "chat.db"

# For py2app bundled apps, resources are in Contents/Resources
# For development, use relative paths from this file
if getattr(sys, 'frozen', False):
    # Running as bundled app
    bundle_dir = Path(sys.executable).parent.parent / "Resources"
    template_dir = bundle_dir / "templates"
    static_dir = bundle_dir / "static"
else:
    # Running in development
    template_dir = Path(__file__).parent / "templates"
    static_dir = Path(__file__).parent / "static"

app = Flask(__name__, template_folder=str(template_dir), static_folder=str(static_dir))

# Enable debug logging to file
import logging
log_path = Path.home() / "Library" / "Logs" / "People.log"
log_path.parent.mkdir(parents=True, exist_ok=True)
logging.basicConfig(
    filename=str(log_path),
    level=logging.DEBUG,
    format='%(asctime)s %(levelname)s: %(message)s'
)
app.logger.setLevel(logging.DEBUG)
app.logger.info(f"App starting, template_dir={template_dir}, static_dir={static_dir}")

# URL regex pattern
URL_PATTERN = re.compile(r'(https?://[^\s<>"\']+)')


def linkify(text: str, max_len: int = 50) -> Markup:
    """Convert URLs in text to clickable links, truncating long URLs."""
    if not text:
        return Markup("")
    
    def replace_url(match):
        url = match.group(1)
        escaped_url = escape(url)
        if len(url) > max_len:
            display_url = escape(url[:max_len]) + "..."
        else:
            display_url = escaped_url
        return f'<a href="{escaped_url}" target="_blank" rel="noopener">{display_url}</a>'
    
    escaped_text = escape(text)
    linked = URL_PATTERN.sub(replace_url, str(escaped_text))
    return Markup(linked)


# Register as Jinja filter
app.jinja_env.filters['linkify'] = linkify

# In-memory state for drafts and "later" marks
drafts: dict[int, str] = {}  # chat_id -> draft text
committed: dict[int, str] = {}  # chat_id -> committed text
later: set[int] = set()  # chat_ids marked as later

# Persistent ignored identifiers (loaded from people.tsv)
ignored: set[str] = set()  # chat_identifiers to permanently ignore


@dataclass
class Attachment:
    filename: str
    mime_type: str
    transfer_name: str
    
    @property
    def is_image(self) -> bool:
        return self.mime_type and self.mime_type.startswith("image/")
    
    @property
    def path(self) -> str:
        """Get the full path, expanding ~."""
        if self.filename:
            return os.path.expanduser(self.filename)
        return ""
    
    @property
    def url(self) -> str:
        """Get the URL to serve this attachment."""
        if self.filename:
            # Extract path relative to ~/Library/Messages/Attachments/
            prefix = "~/Library/Messages/Attachments/"
            if self.filename.startswith(prefix):
                return "/attachment/" + self.filename[len(prefix):]
        return ""


# Reaction type mappings
REACTION_EMOJI = {
    2000: "❤️",   # Loved
    2001: "👍",   # Liked
    2002: "👎",   # Disliked
    2003: "😂",   # Laughed
    2004: "‼️",   # Emphasized
    2005: "❓",   # Questioned
    2006: "🫶",   # New heart reaction
}

# Removal codes (3000 series) - we'll filter these out
REACTION_REMOVALS = {3000, 3001, 3002, 3003, 3004, 3005, 3006}


@dataclass
class Reaction:
    emoji: str
    is_from_me: bool
    sender: Optional[str] = None


@dataclass
class Message:
    rowid: int
    guid: str
    text: str
    date: datetime
    is_from_me: bool
    sender: Optional[str] = None
    attachments: list[Attachment] = field(default_factory=list)
    reactions: list[Reaction] = field(default_factory=list)
    
    @property
    def display_text(self) -> str:
        """Text with attachment placeholders removed."""
        if not self.text:
            return ""
        # Remove Unicode object replacement character (attachment placeholder)
        return self.text.replace('\ufffc', '').strip()
    
    @property
    def is_image_only(self) -> bool:
        """True if message has images but no text."""
        has_image = any(a.is_image for a in self.attachments)
        return has_image and not self.display_text
    
    @property
    def reaction_summary(self) -> str:
        """Get unique reaction emojis as a string."""
        unique = []
        seen = set()
        for r in self.reactions:
            if r.emoji not in seen:
                unique.append(r.emoji)
                seen.add(r.emoji)
        return "".join(unique)


@dataclass
class Conversation:
    chat_id: int
    display_name: Optional[str]
    chat_identifier: str
    style: int  # 43 = group, 45 = 1:1
    unread_count: int
    last_message_date: datetime
    messages: list[Message] = field(default_factory=list)
    participants: list[str] = field(default_factory=list)
    
    @property
    def is_group(self) -> bool:
        return self.style == 43
    
    @property
    def name(self) -> str:
        """Get display name, falling back to identifier or participant names."""
        if self.display_name:
            return self.display_name
        # Try to get from people.tsv or contacts
        name = get_contact_name(self.chat_identifier)
        if name:
            return name
        # For groups, show participant names
        if self.is_group and self.participants:
            names = []
            for p in self.participants[:3]:  # Limit to 3 names
                pname = get_contact_name(p)
                if pname:
                    # Use first name only for brevity
                    names.append(pname.split()[0])
                else:
                    names.append(p)
            result = ", ".join(names)
            if len(self.participants) > 3:
                result += f" +{len(self.participants) - 3}"
            return result
        return self.chat_identifier
    
    @property
    def messages_url(self) -> str:
        """Get URL to open this conversation in Messages.app."""
        # For iMessage, use imessage:// URL scheme
        if self.is_group:
            return f"imessage://?groupID={self.chat_identifier}"
        else:
            return f"imessage://{self.chat_identifier}"


def apple_to_datetime(apple_ts: int) -> datetime:
    """Convert Apple timestamp to datetime."""
    if apple_ts is None:
        return datetime.now()
    # If > 1e12, it's in nanoseconds
    if apple_ts > 1e12:
        apple_ts = apple_ts / 1e9
    unix_ts = apple_ts + APPLE_EPOCH
    return datetime.fromtimestamp(unix_ts)


def parse_attributed_body(blob: bytes) -> str:
    """Extract text from attributedBody blob."""
    if not blob:
        return ""
    
    try:
        # Find NSString marker and extract text after it
        parts = blob.split(b"NSString")
        if len(parts) < 2:
            return ""
        
        text = parts[1][5:]  # Skip 5 bytes after NSString
        
        # Length is either 1 byte or 2 bytes (little-endian)
        length = text[0]
        start = 1
        
        if length == 0x81:  # 129 indicates 2-byte length
            length = int.from_bytes(text[1:3], "little")
            start = 3
        
        return text[start:start + length].decode("utf-8", errors="ignore")
    except (IndexError, ValueError):
        return ""


def get_db_connection():
    """Get read-only connection to chat.db."""
    try:
        conn = sqlite3.connect(f"file:{CHAT_DB}?mode=ro", uri=True)
        conn.row_factory = sqlite3.Row
        return conn
    except sqlite3.OperationalError as e:
        if "unable to open database" in str(e):
            # Open System Settings to Full Disk Access
            subprocess.run([
                "open", 
                "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"
            ])
            raise RuntimeError(
                "Cannot access iMessage database. Please grant Full Disk Access to People.app "
                "in System Settings > Privacy & Security > Full Disk Access, then relaunch."
            ) from e
        raise


# People.tsv cache
_people_cache: dict[str, dict] = {}
_people_loaded = False

# Data directory for writable files (works in both dev and bundled app)
DATA_DIR = Path.home() / "Library" / "Application Support" / "People"
DATA_DIR.mkdir(parents=True, exist_ok=True)


def load_people_tsv():
    """Load people.tsv into cache."""
    global _people_cache, _people_loaded, ignored
    
    people_file = DATA_DIR / "people.tsv"
    if not people_file.exists():
        _people_loaded = True
        return
    
    _people_cache = {}
    ignored = set()
    with open(people_file, "r") as f:
        reader = csv.DictReader(f, delimiter="\t")
        for row in reader:
            if row["identifier"].startswith("#"):
                continue
            _people_cache[row["identifier"]] = row
            # Check if ignored
            if row.get("ignored", "").lower() in ("1", "true", "yes"):
                ignored.add(row["identifier"])
    
    _people_loaded = True


def save_people_tsv():
    """Save people.tsv from cache."""
    people_file = DATA_DIR / "people.tsv"
    
    fieldnames = ["identifier", "display_name", "priority", "ignored", "notes"]
    with open(people_file, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames, delimiter="\t")
        writer.writeheader()
        for identifier, row in _people_cache.items():
            # Ensure all fields exist
            out_row = {k: row.get(k, "") for k in fieldnames}
            out_row["identifier"] = identifier
            if identifier in ignored:
                out_row["ignored"] = "1"
            writer.writerow(out_row)


def get_people_entry(identifier: str) -> Optional[dict]:
    """Get entry from people.tsv."""
    if not _people_loaded:
        load_people_tsv()
    return _people_cache.get(identifier)


# Contacts cache
_contacts_cache: dict[str, str] = {}
_contacts_loaded = False


def load_contacts():
    """Load contacts from macOS Contacts using pyobjc."""
    global _contacts_cache, _contacts_loaded
    
    try:
        import objc
        from Contacts import (
            CNContactStore,
            CNContactFetchRequest,
            CNContactGivenNameKey,
            CNContactFamilyNameKey,
            CNContactPhoneNumbersKey,
            CNContactEmailAddressesKey,
        )
        
        store = CNContactStore.alloc().init()
        keys = [
            CNContactGivenNameKey,
            CNContactFamilyNameKey,
            CNContactPhoneNumbersKey,
            CNContactEmailAddressesKey,
        ]
        request = CNContactFetchRequest.alloc().initWithKeysToFetch_(keys)
        
        def handler(contact, stop):
            given = contact.givenName() or ""
            family = contact.familyName() or ""
            name = f"{given} {family}".strip()
            
            if not name:
                return
            
            # Map phone numbers
            for phone in contact.phoneNumbers():
                number = phone.value().stringValue()
                # Normalize: remove spaces, dashes, parens
                normalized = "".join(c for c in number if c.isdigit() or c == "+")
                if normalized:
                    _contacts_cache[normalized] = name
                    # Also store without country code for US numbers
                    if normalized.startswith("+1") and len(normalized) == 12:
                        _contacts_cache[normalized[2:]] = name
            
            # Map emails
            for email in contact.emailAddresses():
                addr = email.value()
                if addr:
                    _contacts_cache[addr.lower()] = name
        
        store.enumerateContactsWithFetchRequest_error_usingBlock_(
            request, None, handler
        )
        _contacts_loaded = True
        
    except ImportError:
        print("Warning: pyobjc not installed, contact names won't be resolved")
        _contacts_loaded = True
    except Exception as e:
        print(f"Warning: Could not load contacts: {e}")
        _contacts_loaded = True


def get_contact_name(identifier: str) -> Optional[str]:
    """Get contact name from people.tsv or macOS Contacts."""
    # First check people.tsv
    entry = get_people_entry(identifier)
    if entry and entry.get("display_name"):
        return entry["display_name"]
    
    # Then check Contacts
    if not _contacts_loaded:
        load_contacts()
    
    # Try direct lookup
    if identifier in _contacts_cache:
        return _contacts_cache[identifier]
    
    # Try normalized phone number
    normalized = "".join(c for c in identifier if c.isdigit() or c == "+")
    if normalized in _contacts_cache:
        return _contacts_cache[normalized]
    
    # Try without +1 prefix
    if normalized.startswith("+1"):
        if normalized[2:] in _contacts_cache:
            return _contacts_cache[normalized[2:]]
    
    return None


def get_priority(identifier: str) -> int:
    """Get priority from people.tsv (default 5)."""
    entry = get_people_entry(identifier)
    if entry and entry.get("priority"):
        try:
            return int(entry["priority"])
        except ValueError:
            pass
    return 5


def get_unread_conversations() -> list[Conversation]:
    """Get all conversations with unread messages."""
    conn = get_db_connection()
    
    # Get conversations with unread counts
    cursor = conn.execute("""
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
          AND m.is_finished = 1
          AND c.is_filtered != 2
        GROUP BY c.ROWID
        ORDER BY last_message_date DESC
    """)
    
    conversations = []
    for row in cursor:
        conv = Conversation(
            chat_id=row["chat_id"],
            display_name=row["display_name"],
            chat_identifier=row["chat_identifier"],
            style=row["style"],
            unread_count=row["unread_count"],
            last_message_date=apple_to_datetime(row["last_message_date"]),
        )
        conversations.append(conv)
    
    # Load participants first (needed for group name resolution)
    for conv in conversations:
        if conv.is_group:
            cursor = conn.execute("""
                SELECT h.id
                FROM handle h
                JOIN chat_handle_join chj ON h.ROWID = chj.handle_id
                WHERE chj.chat_id = ?
            """, (conv.chat_id,))
            conv.participants = [row["id"] for row in cursor]
    
    # Load messages for each conversation
    for conv in conversations:
        cursor = conn.execute("""
            SELECT 
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
            LIMIT 15
        """, (conv.chat_id,))
        
        messages_by_guid = {}
        
        for row in cursor:
            text = row["text"]
            if not text and row["attributedBody"]:
                text = parse_attributed_body(row["attributedBody"])
            
            # Load attachments if present
            attachments = []
            if row["cache_has_attachments"]:
                att_cursor = conn.execute("""
                    SELECT a.filename, a.mime_type, a.transfer_name
                    FROM attachment a
                    JOIN message_attachment_join maj ON a.ROWID = maj.attachment_id
                    WHERE maj.message_id = ?
                """, (row["ROWID"],))
                for att_row in att_cursor:
                    if att_row["filename"]:
                        attachments.append(Attachment(
                            filename=att_row["filename"],
                            mime_type=att_row["mime_type"] or "",
                            transfer_name=att_row["transfer_name"] or "",
                        ))
            
            # Include message if it has text or attachments
            if (text and text.strip()) or attachments:
                # Resolve sender to contact name for group messages
                sender_id = row["sender"]
                sender_name = None
                if sender_id and conv.is_group:
                    sender_name = get_contact_name(sender_id)
                    if not sender_name:
                        # Use short form of phone/email if no contact
                        sender_name = sender_id
                
                msg = Message(
                    rowid=row["ROWID"],
                    guid=row["guid"],
                    text=text or "",
                    date=apple_to_datetime(row["date"]),
                    is_from_me=bool(row["is_from_me"]),
                    sender=sender_name,
                    attachments=attachments,
                )
                conv.messages.append(msg)
                messages_by_guid[row["guid"]] = msg
        
        # Load reactions for these messages
        if messages_by_guid:
            guid_list = list(messages_by_guid.keys())
            # Build list of possible associated_message_guid formats: "p:0/GUID", "p:1/GUID", etc.
            prefixed_guids = []
            for guid in guid_list:
                prefixed_guids.append(f"p:0/{guid}")
                prefixed_guids.append(f"p:1/{guid}")
                prefixed_guids.append(f"bp:{guid}")
            
            placeholders = ",".join("?" * len(prefixed_guids))
            reaction_cursor = conn.execute(f"""
                SELECT 
                    m.associated_message_guid,
                    m.associated_message_type,
                    m.is_from_me,
                    h.id as sender
                FROM message m
                LEFT JOIN handle h ON m.handle_id = h.ROWID
                WHERE m.associated_message_guid IN ({placeholders})
                  AND m.associated_message_type IN (2000, 2001, 2002, 2003, 2004, 2005, 2006)
            """, prefixed_guids)
            
            for r_row in reaction_cursor:
                # associated_message_guid format: "p:0/GUID" or "bp:GUID"
                assoc_guid = r_row["associated_message_guid"]
                if assoc_guid.startswith("p:"):
                    # Extract GUID after "p:0/" or similar
                    parts = assoc_guid.split("/", 1)
                    if len(parts) == 2:
                        target_guid = parts[1]
                    else:
                        continue
                elif assoc_guid.startswith("bp:"):
                    target_guid = assoc_guid[3:]
                else:
                    target_guid = assoc_guid
                
                if target_guid in messages_by_guid:
                    emoji = REACTION_EMOJI.get(r_row["associated_message_type"], "")
                    if emoji:
                        sender_name = None
                        if r_row["sender"] and conv.is_group:
                            sender_name = get_contact_name(r_row["sender"]) or r_row["sender"]
                        
                        messages_by_guid[target_guid].reactions.append(Reaction(
                            emoji=emoji,
                            is_from_me=bool(r_row["is_from_me"]),
                            sender=sender_name,
                        ))
        
        # Reverse to chronological order
        conv.messages.reverse()
    
    conn.close()
    
    # Sort by priority and recency (most recent first)
    def sort_key(c):
        priority = get_priority(c.chat_identifier)
        return (priority, -c.last_message_date.timestamp())
    
    conversations.sort(key=sort_key)
    
    return conversations


def mark_as_read(chat_identifier: str) -> bool:
    """Mark all messages in a chat as read by updating the database."""
    try:
        conn = sqlite3.connect(CHAT_DB)
        cursor = conn.cursor()
        cursor.execute("""
            UPDATE message SET is_read = 1
            WHERE ROWID IN (
                SELECT m.ROWID FROM message m
                JOIN chat_message_join cmj ON m.ROWID = cmj.message_id
                JOIN chat c ON cmj.chat_id = c.ROWID
                WHERE c.chat_identifier = ? AND m.is_read = 0
            )
        """, (chat_identifier,))
        conn.commit()
        conn.close()
        return True
    except Exception as e:
        print(f"Error marking as read: {e}")
        return False


def send_message(chat_identifier: str, text: str, is_group: bool = False) -> bool:
    """Send a message via AppleScript and mark as read."""
    # Escape quotes and backslashes in text
    escaped_text = text.replace("\\", "\\\\").replace('"', '\\"')
    
    # Build the full chat ID that Messages.app expects
    # Format: "any;+;chatXXX" for groups, "any;-;+1234567890" for 1:1
    if is_group:
        full_chat_id = f"any;+;{chat_identifier}"
    else:
        full_chat_id = f"any;-;{chat_identifier}"
    
    script = f'''
    tell application "Messages"
        set targetChat to chat id "{full_chat_id}"
        send "{escaped_text}" to targetChat
    end tell
    '''
    
    try:
        result = subprocess.run(
            ["osascript", "-e", script],
            capture_output=True,
            text=True,
            timeout=10,
        )
        if result.returncode != 0:
            print(f"AppleScript error: {result.stderr}")
            return False
        
        # Mark messages as read after successful send
        mark_as_read(chat_identifier)
        return True
    except Exception as e:
        print(f"Error sending message: {e}")
        return False


def calculate_grid_cols(count: int, sidebar_width: int = 196) -> int:
    """Calculate optimal grid columns for near-square cells."""
    if count == 0:
        return 1
    # Assume roughly square aspect ratio for sidebar grid area
    # sidebar_width minus padding, gap=3px
    import math
    # Target: cells that are roughly square
    # With 196px width and 3px gaps, try different column counts
    best_cols = 1
    best_ratio = float('inf')
    
    for cols in range(1, count + 1):
        rows = math.ceil(count / cols)
        cell_w = (sidebar_width - 3 * (cols - 1)) / cols
        # Estimate available height as similar to width for the grid area
        cell_h = cell_w  # Aim for square
        if cell_w < 8:  # Too small
            break
        ratio = max(cols / rows, rows / cols) if rows > 0 else float('inf')
        if ratio < best_ratio:
            best_ratio = ratio
            best_cols = cols
    
    return best_cols


# Cache directory for converted images
CACHE_DIR = Path.home() / "Library" / "Caches" / "People"
CACHE_DIR.mkdir(parents=True, exist_ok=True)


@app.route("/attachment/<path:filepath>")
def serve_attachment(filepath):
    """Serve an attachment file from ~/Library/Messages/Attachments."""
    # Security: only allow files under the Messages Attachments directory
    attachments_dir = Path.home() / "Library" / "Messages" / "Attachments"
    full_path = attachments_dir / filepath
    
    # Resolve to prevent path traversal
    try:
        full_path = full_path.resolve()
        if not str(full_path).startswith(str(attachments_dir.resolve())):
            abort(403)
    except (ValueError, OSError):
        abort(404)
    
    if not full_path.exists():
        abort(404)
    
    # Convert HEIC to JPEG for browser compatibility
    if full_path.suffix.lower() in ('.heic', '.heif'):
        # Create a cached JPEG version
        cache_key = filepath.replace('/', '_').replace('.', '_') + '.jpg'
        cached_path = CACHE_DIR / cache_key
        
        if not cached_path.exists():
            # Convert using sips (macOS built-in)
            try:
                subprocess.run(
                    ['sips', '-s', 'format', 'jpeg', '-s', 'formatOptions', '80',
                     str(full_path), '--out', str(cached_path)],
                    capture_output=True,
                    timeout=10,
                )
            except Exception:
                abort(500)
        
        if cached_path.exists():
            return send_file(cached_path, mimetype='image/jpeg')
    
    return send_file(full_path)


@app.route("/")
def index():
    """Main page."""
    conversations = get_unread_conversations()
    
    # Calculate stats
    total = len(conversations)
    later_count = len([c for c in conversations if c.chat_id in later])
    committed_count = len(committed)
    ready_count = committed_count
    remaining = total - later_count
    
    # Calculate grid columns
    grid_cols = calculate_grid_cols(total)
    
    return render_template(
        "index.html",
        conversations=conversations,
        drafts=drafts,
        committed=committed,
        later=later,
        ignored=ignored,
        total=total,
        ready_count=ready_count,
        remaining=remaining,
        grid_cols=grid_cols,
    )


@app.route("/api/draft", methods=["POST"])
def save_draft():
    """Save a draft message."""
    data = request.json
    chat_id = int(data["chat_id"])
    text = data.get("text", "").strip()
    
    # Remove from committed if editing
    if chat_id in committed:
        del committed[chat_id]
    
    if text:
        drafts[chat_id] = text
    elif chat_id in drafts:
        del drafts[chat_id]
    
    return jsonify({"status": "ok", "state": "draft" if text else "empty"})


@app.route("/api/commit", methods=["POST"])
def commit_message():
    """Commit a draft message."""
    data = request.json
    chat_id = int(data["chat_id"])
    text = data.get("text", "").strip()
    
    if text:
        committed[chat_id] = text
        if chat_id in drafts:
            del drafts[chat_id]
        return jsonify({"status": "ok", "state": "committed"})
    
    return jsonify({"status": "error", "message": "No text provided"})


@app.route("/api/later", methods=["POST"])
def mark_later():
    """Mark a conversation as 'later'."""
    data = request.json
    chat_id = int(data["chat_id"])
    
    if chat_id in later:
        later.remove(chat_id)
        return jsonify({"status": "ok", "is_later": False})
    else:
        later.add(chat_id)
        # Remove from drafts/committed
        drafts.pop(chat_id, None)
        committed.pop(chat_id, None)
        return jsonify({"status": "ok", "is_later": True})


@app.route("/api/ignore", methods=["POST"])
def mark_ignore():
    """Mark a conversation as permanently ignored (saved to people.tsv)."""
    data = request.json
    chat_identifier = data["chat_identifier"]
    
    if chat_identifier in ignored:
        ignored.remove(chat_identifier)
        if chat_identifier in _people_cache:
            _people_cache[chat_identifier]["ignored"] = ""
        is_ignored = False
    else:
        ignored.add(chat_identifier)
        if chat_identifier not in _people_cache:
            _people_cache[chat_identifier] = {"identifier": chat_identifier}
        _people_cache[chat_identifier]["ignored"] = "1"
        is_ignored = True
    
    save_people_tsv()
    return jsonify({"status": "ok", "is_ignored": is_ignored})


_sending = False

@app.route("/api/send-all", methods=["POST"])
def send_all():
    """Send all committed messages."""
    global _sending
    
    if _sending:
        return jsonify({"status": "ok", "results": [], "skipped": True})
    
    _sending = True
    try:
        # Copy and clear committed dict atomically
        to_send = dict(committed)
        committed.clear()
        
        conversations = get_unread_conversations()
        conv_map = {c.chat_id: c for c in conversations}
        
        results = []
        for chat_id, text in to_send.items():
            conv = conv_map.get(chat_id)
            if conv:
                success = send_message(
                    conv.chat_identifier,
                    text,
                    is_group=conv.is_group,
                )
                results.append({
                    "chat_id": chat_id,
                    "success": success,
                    "name": conv.name,
                })
        
        return jsonify({"status": "ok", "results": results})
    finally:
        _sending = False


@app.route("/api/refresh", methods=["POST"])
def refresh():
    """Refresh conversations (non-destructive)."""
    global later
    
    conversations = get_unread_conversations()
    current_chat_ids = {c.chat_id for c in conversations}
    
    # Remove drafts/committed/later for conversations that no longer exist
    for chat_id in list(drafts.keys()):
        if chat_id not in current_chat_ids:
            del drafts[chat_id]
    
    for chat_id in list(committed.keys()):
        if chat_id not in current_chat_ids:
            del committed[chat_id]
    
    later = later & current_chat_ids
    
    return jsonify({"status": "ok"})


@app.route("/api/stats")
def get_stats():
    """Get current stats for progress bar."""
    conversations = get_unread_conversations()
    total = len(conversations)
    later_count = len([c for c in conversations if c.chat_id in later])
    remaining = total - later_count
    ready_count = len(committed)
    
    return jsonify({
        "total": total,
        "remaining": remaining,
        "ready": ready_count,
        "later": later_count,
        "drafts": len(drafts),
    })


if __name__ == "__main__":
    # Preload data
    load_people_tsv()
    load_contacts()
    
    print(f"Starting iMessage Inbox Crusher...")
    print(f"Database: {CHAT_DB}")
    
    app.run(debug=True, port=5050)
