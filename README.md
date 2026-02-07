<div align="center">

# OpenWispr

[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS-lightgrey.svg)](https://github.com/abhaymundhara/OpenWispr)
<img src="https://visitor-badge.laobi.icu/badge?page_id=HKUDS.OpenWispr&style=for-the-badge&color=00d4ff" alt="Views">

</div>

OpenWispr is a privacy-first desktop dictation utility that runs locally and is built with Tauri (Rust + web UI). It provides a minimal, always-on-top floating dictation pill that captures microphone audio on-device and emits audio levels to a small React UI, while delegating transcription to a pluggable STT adapter layer.

Supported platforms

- macOS (primary)
- Windows

Project goals

- Local-first transcription: prefer on-device models to preserve privacy.
- Minimal UX: small floating waveform pill with one global hotkey for dictation.
- Extensible STT adapters: trait-based architecture to add model backends.
- Local whisper.cpp transcription with automatic model caching.

Highlights

- Small, focused UI: React + Tailwind floating pill with real-time audio level visualization.
- Native audio capture: `cpal` in Rust for microphone capture and RMS → dB normalization.
- Pluggable STT layer: `crates/stt` defines `SttAdapter` and platform adapters backed by `whisper.cpp`.
- Tauri-based: Rust backend handles system integration and emits events to the frontend.

Repository layout

- `apps/desktop/` — Tauri + React app
  - `src/` — React UI and event handling
  - `src-tauri/` — Rust backend: audio capture, hotkey listeners, STT integration points
  - `src-tauri/tauri.conf.json` — Tauri settings and bundle configuration
- `crates/stt/` — STT trait and platform adapters

Quick start (development)

Prerequisites

- Node.js (v18+ recommended) and `pnpm`
- Rust toolchain (stable) with cargo

Run the app (dev)

```bash
# from repo root
cd apps/desktop
pnpm install
pnpm tauri dev
```
