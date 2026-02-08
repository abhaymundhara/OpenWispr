Docs: Build & run (scaffold)

Prerequisites (developer):

- Rust + cargo
- Node.js + pnpm
- Xcode (macOS) for Metal hardware acceleration
- [Vulkan SDK](https://vkr.org/sdk/home/) (Windows) for GPU acceleration
- [LLVM](https://github.com/llvm/llvm-project/releases) (Windows) for `bindgen` (ensure "Add to PATH" is checked)
- Visual Studio Build Tools 2022 (Windows) with "Desktop development with C++" workload

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
