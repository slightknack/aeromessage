# iMessage Inbox Crusher - Initial Design

## Core Concept

"Superhuman for iMessage" - batch process your unread messages in one focused session.

## Layout

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                                                                             │
│  ┌─────────────────────┐  ┌───────────────────────────────────────────────┐ │
│  │                     │  │                                               │ │
│  │  CONVERSATION GRID  │  │              MESSAGE STREAM                   │ │
│  │                     │  │                                               │ │
│  │  ┌──┬──┬──┬──┬──┐   │  │  ┌─────────────────────────────────────────┐  │ │
│  │  │▓▓│░░│▓▓│░░│▓▓│   │  │  │ CONVERSATION 1                          │  │ │
│  │  ├──┼──┼──┼──┼──┤   │  │  │                                         │  │ │
│  │  │░░│▓▓│░░│▓▓│░░│   │  │  │  them: Hey, are you coming tonight?     │  │ │
│  │  ├──┼──┼──┼──┼──┤   │  │  │  them: Let me know!                     │  │ │
│  │  │▓▓│░░│▓▓│░░│▓▓│   │  │  │                                         │  │ │
│  │  ├──┼──┼──┼──┼──┤   │  │  │  you (last): I'll check my calendar     │  │ │
│  │  │░░│▓▓│░░│▓▓│░░│   │  │  │                                         │  │ │
│  │  └──┴──┴──┴──┴──┘   │  │  │  ┌─────────────────────────────────┐    │  │ │
│  │                     │  │  │  │ Type reply...              [⏎] │    │  │ │
│  │  Grid Legend:       │  │  │  └─────────────────────────────────┘    │  │ │
│  │  ░░ = needs reply   │  │  │                           [Mark Later] │  │ │
│  │  ▓▓ = draft ready   │  │  └─────────────────────────────────────────┘  │ │
│  │  ██ = committed     │  │                                               │ │
│  │  ·· = marked later  │  │  ┌─────────────────────────────────────────┐  │ │
│  │                     │  │  │ CONVERSATION 2                          │  │ │
│  ├─────────────────────┤  │  │                                         │  │ │
│  │                     │  │  │  them: Did you see the news?            │  │ │
│  │  ████████████░░░░░  │  │  │                                         │  │ │
│  │  12/20 ready        │  │  │  ┌─────────────────────────────────┐    │  │ │
│  │                     │  │  │  │ [DRAFT] Yeah, crazy right?  [⏎] │    │  │ │
│  ├─────────────────────┤  │  │  └─────────────────────────────────┘    │  │ │
│  │                     │  │  │                           [Mark Later] │  │ │
│  │  ┌─────────────────┐│  │  └─────────────────────────────────────────┘  │ │
│  │  │   SEND ALL  ▼   ││  │                                               │ │
│  │  └─────────────────┘│  │  ┌─────────────────────────────────────────┐  │ │
│  │  [↻ Refresh]        │  │  │ CONVERSATION 3 (Group: Team Lunch)      │  │ │
│  │                     │  │  │                                         │  │ │
│  └─────────────────────┘  │  │  Alice: Where should we go?             │  │ │
│                           │  │  Bob: How about tacos?                  │  │ │
│                           │  │  Alice: +1                              │  │ │
│                           │  │                                         │  │ │
│                           │  │  ┌─────────────────────────────────┐    │  │ │
│                           │  │  │ Type reply...              [⏎] │    │  │ │
│                           │  │  └─────────────────────────────────┘    │  │ │
│                           │  │                           [Mark Later] │  │ │
│                           │  └─────────────────────────────────────────┘  │ │
│                           │                                               │ │
│                           └───────────────────────────────────────────────┘ │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Left Panel (1/3 width)

### Conversation Grid
- Grid of colored squares, one per conversation with unread messages
- Color states:
  - **Red/Orange** (`░░`) - Needs reply, no draft
  - **Yellow** (`▓▓`) - Has draft (not yet committed)
  - **Green** (`██`) - Committed (ready to send)
  - **Gray/Dim** (`··`) - Marked as "Later" (excluded from progress)
- Clicking a square scrolls the message stream to that conversation
- Squares ordered by priority (importance + time waiting)

### Progress Bar
- Shows: `X/Y ready` where Y excludes "Later" items
- Visual progress bar fills as messages are committed
- Updates in real-time as you work

### Action Buttons
- **SEND ALL** - Big blue button
  - Disabled (grayed) until progress bar is full
  - Has dropdown arrow (▼) to force-send even if incomplete
  - Sends all committed messages, then refreshes
- **Refresh** - Next to Send All
  - Non-destructive: preserves drafts for conversations still present
  - Clears sent messages and adds new unread conversations

## Right Panel (2/3 width)

### Message Stream
- Vertically scrolling list of conversation cards
- Each card shows:
  - **Header**: Contact name (from macOS Contacts) or phone/email
  - **Context**: Last few messages (yours + theirs) for context
  - **Reply Box**: Text input for your reply
  - **Later Button**: Marks conversation as "later"

### Reply Box States
1. **Empty** - Placeholder text, needs reply
2. **Draft** - Yellow border, text entered but not committed
3. **Committed** - Green border, press Enter to commit
4. **Later** - Grayed out, dimmed card

### Keyboard Navigation
- **Tab** - Jump to next conversation needing reply (empty or draft)
- **Enter** - Commit current draft, move to next
- **Escape** - Clear current draft
- **Cmd+Enter** - Send all (same as clicking Send All)

## Message States

```
Empty → [type text] → Draft → [Enter] → Committed → [edit] → Draft
                        ↓
                   [Mark Later] → Later
```

## Data Flow

1. **On Load**: Snapshot all unread conversations from chat.db
2. **While Working**: 
   - Drafts stored in memory (and localStorage for persistence)
   - Grid updates as states change
3. **On Send All**:
   - Send each committed message via AppleScript/osascript
   - Refresh to get new state
4. **On Refresh**:
   - Re-query chat.db
   - Preserve drafts for conversations that still exist
   - Add new unread conversations
   - Remove conversations that are now read

## Priority Ordering

Conversations sorted by:
1. Manual priority from `people.tsv` (if set)
2. Time waiting (older = higher priority)
3. Message count (more unread = higher priority)

## people.tsv Schema

```tsv
identifier	display_name	priority	notes
+14155551234	Mom	1	Always respond quickly
john@example.com	John Smith	2	Work contact
chat913493738319742936	The Family	3	Group chat
```

- `identifier`: Phone, email, or chat GUID
- `display_name`: Override for contact name
- `priority`: 1-5 (1 = highest)
- `notes`: Optional notes for yourself

## Technical Stack

- **Backend**: Flask (Python)
- **Frontend**: Jinja2 templates + HTMX
- **Database**: Read-only access to ~/Library/Messages/chat.db
- **Contacts**: Integration with macOS Contacts via pyobjc or sqlite
- **Sending**: AppleScript via osascript for sending messages
