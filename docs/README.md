Docs: Build & run (scaffold)

Prerequisites (developer):
- Rust + cargo
- Node.js + pnpm
- Xcode (macOS) for notarization/packaging

Frontend (desktop) — quick start (development):
```bash
cd apps/desktop
pnpm install
pnpm dev
```

Tauri (Rust) backend (development):
```bash
cd apps/desktop/src-tauri
cargo build
```

Notes:
- This scaffold provides minimal stubs for the Tauri app and Rust crates.
- Model weights are intentionally not included.
