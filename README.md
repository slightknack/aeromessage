# Aeromessage

*Batch-reply to iMessages, beautifully.*

![Screenshot](example.png)

A native macOS app for processing your unread messages in one sitting. Draft replies, commit them, then send all at once.

## Features

- Grid view of all unread conversations
- Draft and commit replies before sending
- Later / Ignore / Mark as Read to triage
- Privacy mode to blur content
- Send All when ready

## Install

Download the latest DMG from [Releases](https://github.com/slightknack/aeromessage/releases), or build from source:

```sh
cd oxidized
./build.sh
```

The DMG will be at `oxidized/out/Aeromessage.dmg`.

On first launch, grant **Full Disk Access** in System Settings to read your messages.

## Development

```sh
cd oxidized
npx @tauri-apps/cli dev
```

Or with Nix:

```sh
nix develop
cd oxidized
cargo tauri dev
```

## License

CC0 1.0 Universal - Public Domain
