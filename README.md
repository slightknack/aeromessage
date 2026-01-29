---
model:  claude-opus-4-5
date:   2026-01-23
driver: Isaac Clayton
---

# Aeromessage

*Batch-reply to iMessages, beautifully.*

A native macOS app for processing your unread messages in one sitting. Draft replies, commit them, then send all at once.

![Screenshot](https://github.com/slightknack/aeromessage/raw/master/src/static/bg.jpg)

## Why?

Sometimes you open Messages and there are 47 unread conversations staring back at you. Aeromessage turns inbox anxiety into inbox zero.

## Features

- **Grid view** of all unread conversations at a glance
- **Draft and commit** replies before sending
- **Reactions** displayed inline on message bubbles
- **Later/Ignore/Read** to triage what doesn't need a response
- **Send All** when you're ready
- **Contact resolution** from macOS Contacts

## Install (Rust/Tauri)

Requires macOS 10.15+ and Rust.

```sh
git clone https://github.com/slightknack/aeromessage.git
cd aeromessage/oxidized
./build.sh
```

The DMG will be at `oxidized/out/Aeromessage.dmg`. Open it and drag to Applications.

On first launch, grant **Full Disk Access** in System Settings to read your messages.

### Development

```sh
cd oxidized
npx @tauri-apps/cli dev
```

## Legacy (Python/Flask)

There's also an older Python version in `src/`. Requires Python 3.10+.

```sh
python3 -m venv venv && source venv/bin/activate
pip install -r requirements.txt
./run
```

Opens in your browser at `localhost:5050`.

## License

CC0 1.0 Universal - Public Domain
