<div align="center">

# OpenWispr

[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS-lightgrey.svg)](https://github.com/abhaymundhara/OpenWispr)
<img src="https://visitor-badge.laobi.icu/badge?page_id=HKUDS.OpenWispr&style=for-the-badge&color=00d4ff" alt="Views">

**OpenWispr is a local-first desktop dictation app built with Tauri (Rust backend + React frontend).**

</div>

## Features

- Local transcription with Whisper-compatible models
- Floating dictation pill UI
- Menu bar (macOS) / system tray (Windows) app behavior
- Model Manager window for downloading and selecting models
- Personal clipboard preservation while auto-pasting transcription

## Platform Support

- macOS (primary)
- Windows

## Repository Structure

- `apps/desktop` - Tauri desktop app (frontend + native runtime)
- `apps/desktop/src` - React UI
- `apps/desktop/src-tauri` - Rust runtime, hotkey listeners, audio capture, paste pipeline
- `crates/stt` - STT adapter abstraction and Whisper backend
- `docs` - additional project docs

## Prerequisites

- Node.js 18+ (Node 20 LTS recommended)
- `pnpm`
- Rust stable toolchain (`rustup`, `cargo`)
- Platform toolchain:
  - macOS: Xcode Command Line Tools
  - Windows: Visual Studio Build Tools (MSVC) + Windows SDK

## Install

```bash
cd apps/desktop
pnpm install
```

## Run (Development)

```bash
cd apps/desktop
pnpm tauri dev
```

What to expect:

- No main dashboard window opens on launch
- App runs in menu bar (macOS) or system tray (Windows)
- Hold `Fn` to start dictation

## Build (Production)

```bash
cd apps/desktop
pnpm tauri build
```

## Verification Commands

Frontend:

```bash
cd apps/desktop
pnpm build
```

Backend:

```bash
cd apps/desktop/src-tauri
cargo check
cargo test
```

## First-Run Permissions

### macOS

Grant these permissions to OpenWispr/Terminal while developing:

- Microphone
- Accessibility (for global key events and key injection)
- Automation (if prompted, for System Events focus restore)

### Windows

- Microphone access
- Accessibility/input permissions as required by your security policy

## Models

- Models are downloaded automatically when selected/downloaded in Model Manager.
- Default local model cache:
  - macOS: `~/.cache/openwispr/models`
  - Windows: `%LOCALAPPDATA%\\OpenWispr\\models`

## Helpful Environment Variables

- `OPENWISPR_MODEL_DIR` - custom model directory
- `OPENWISPR_INPUT_DEVICE` - force a specific input device name match
- `OPENWISPR_FFMPEG_BIN` - custom ffmpeg binary path
- `OPENWISPR_RAWINPUT_DEBUG=1` (Windows) - log raw keyboard input
- `OPENWISPR_FN_VKEY` / `OPENWISPR_FN_MAKECODE` (Windows) - override Fn mapping

## Troubleshooting

- App seems duplicated or stale in dev:
  - `pkill -f "openwispr-desktop-tauri|vite|pnpm tauri dev" || true`
  - then re-run `pnpm tauri dev`
- Tray/menu icon appears but no window:
  - click tray/menu icon to open Model Manager
- No transcription text:
  - verify microphone permissions and selected model download
- Not pasting into target app:
  - verify Accessibility/Automation permissions and keep target text field focused before dictation

## License

Licensed under the Apache License 2.0. See `LICENSE`.
