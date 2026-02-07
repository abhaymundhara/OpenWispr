<div align="center">

# OpenWispr

[![Platform](https://img.shields.io/badge/platform-%20Windows%20%7C%20macOS%20%)](https://github.com/abhaymundhara/OpenWispr)
<img src="https://visitor-badge.laobi.icu/badge?page_id=HKUDS.OpenWispr&style=for-the-badge&color=00d4ff" alt="Views">

OpenWispr is a privacy-first desktop dictation utility that runs locally and is built with Tauri (Rust + web UI). It provides a minimal, always-on-top floating dictation pill that captures microphone audio on-device and emits audio levels to a small React UI, while delegating transcription to a pluggable STT adapter layer.

Supported (development) platforms

- macOS (primary)
- Windows (adapter work in progress)

Project goals

- Local-first transcription: prefer on-device models to preserve privacy.
- Minimal UX: small floating waveform pill with one global hotkey for dictation.
- Extensible STT adapters: trait-based architecture to add model backends.

Highlights

- Small, focused UI: React + Tailwind floating pill with real-time audio level visualization.
- Native audio capture: `cpal` in Rust for microphone capture and RMS → dB normalization.
- Pluggable STT layer: `crates/stt` defines `SttAdapter` and adapters for platform-specific runtimes.
- Tauri-based: Rust backend handles system integration and emits events to the frontend.

Repository layout

- `apps/desktop/` — Tauri + React app
  - `src/` — React UI and event handling
  - `src-tauri/` — Rust backend: audio capture, hotkey listeners, STT integration points
  - `src-tauri/tauri.conf.json` — Tauri settings and bundle configuration
- `crates/stt/` — STT trait and adapter stubs

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

Notes

- `pnpm tauri dev` starts the Vite dev server and spawns the Tauri backend. Native logs (Rust) appear in the terminal; for macOS native crash logs, use Console.app.

Build (macOS release example)

```bash
# from repo root
cd apps/desktop
pnpm build           # build renderer
cd src-tauri
cargo build --release
# copy the binary into the app bundle (project scaffolding provided)
cp target/release/openwispr-desktop-tauri target/release/bundle/macos/OpenWispr.app/Contents/MacOS/OpenWispr
open target/release/bundle/macos/OpenWispr.app
```

If you plan to distribute signed builds, follow the standard macOS code signing and notarization steps or integrate `pnpm tauri build` with proper signing identities and entitlements.

Hotkey & permissions (macOS)

- macOS requires Accessibility permission for low-level keyboard hooks and Microphone permission for capture. The app prompts when run from a packaged `.app`. If the prompt does not appear, add the app in System Settings → Privacy & Security → Accessibility and Microphone.
- The code separates platform-specific handlers under `src-tauri/src/` (e.g., `fn_key_macos.rs`) to avoid thread/dispatch issues with HIToolbox/TSM APIs.

Troubleshooting

- Crash on Fn key: this is typically caused by calling HIToolbox/TSM APIs from a non-main thread. Check Console.app for crash traces referencing `TSMGetInputSourceProperty` or `dispatch_assert_queue`.
- No audio: verify microphone permissions and your default input device. The backend uses `cpal` and emits `audio-level` events.
- Frontend problems: check the Vite dev server output (terminal) and Tauri logs.

STT adapters

- `crates/stt` implements `SttAdapter` trait and provides a `create_adapter()` factory. Current adapters are scaffolds for MLX (macOS) and Whisper (Windows). Integrating a runtime (e.g., `mlx-whisper`, `whisper.cpp`, or optimized bindings) is required to enable full transcription locally.

Contributing

- Open an issue or PR for bugs, features, or architecture changes.
- Workflow:
  1.  Fork the repository and create a feature branch.
  2.  Implement changes with clear commit messages and tests where applicable.
  3.  Submit a PR with a concise description and rationale.

Roadmap / Next steps

- Implement MLX adapter for macOS model runtime.
- Integrate whisper.cpp/whisper-rs for Windows support.
- Add configurable hotkey and a settings UI.
- Add CI for building and signing macOS releases.

License
See `LICENSE` for license terms.

---

If you want, I can add:

- Badges (CI/build/status), a `CONTRIBUTING.md`, and example screenshots.
- A short `docs/` page with architecture diagrams and developer notes.

Would you like me to also create a `CONTRIBUTING.md` and add CI build badges to the README?
