OpenWispr — Open-source WisprFlow-like Starter

This repository is a scaffolding for an open-source, privacy-first voice dictation product.

Goals included in this scaffold:

- Tauri desktop app (Rust backend + React/TypeScript frontend)
- Local STT adapters under `crates/stt/` (no model weights included)
- Clear platform separation under `crates/platform-*`
- UI components under `ui/`

Deliverables (scaffold):

- Empty app builds on macOS + Windows (scaffolded)
- Onboarding screen launches
- Global shortcut captured and forwarded to UI

See `docs/README.md` for build and run instructions.
