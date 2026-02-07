# STT Adapter Layer

Platform-specific Speech-to-Text adapters for OpenWispr.

## Architecture

```
stt/
├── src/
│   ├── lib.rs              # Core trait & factory
│   └── adapters/
│       ├── mod.rs          # Platform module selector
│       ├── mlx.rs          # macOS (Apple Silicon) via MLX
│       └── whisper.rs      # Windows via whisper.cpp
```

## Platform Support

### macOS (Apple Silicon)

- **Engine**: MLX Whisper
- **Optimization**: Metal GPU + Neural Engine
- **Status**: Stub (ready for integration)
- **Next Steps**:
  - Integrate `mlx-whisper` Python package via PyO3
  - Or use Swift/Objective-C bridge to MLX framework
  - Download models to `~/.cache/openwispr/models/`

### Windows

- **Engine**: whisper.cpp
- **Optimization**: CUDA (NVIDIA) or CPU
- **Status**: Stub (ready for integration)
- **Next Steps**:
  - Add `whisper-rs` crate for Rust bindings
  - Or use direct FFI to whisper.cpp library
  - Download GGML models to `%APPDATA%/openwispr/models/`

## Usage

```rust
use stt::{create_adapter, SttConfig, AudioFormat};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create platform-specific adapter
    let mut adapter = create_adapter()?;

    // Initialize with config
    let config = SttConfig {
        model_name: "base".to_string(),
        ..Default::default()
    };
    adapter.initialize(config).await?;

    // Transcribe audio
    let audio_data: Vec<f32> = vec![/* 16kHz mono audio */];
    let format = AudioFormat::default();
    let result = adapter.transcribe(&audio_data, format).await?;

    println!("Transcription: {}", result.text);
    Ok(())
}
```

## Model Sizes

| Model  | Parameters | English | Multilingual | Size    |
| ------ | ---------- | ------- | ------------ | ------- |
| tiny   | 39 M       | ✓       | ✓            | ~75 MB  |
| base   | 74 M       | ✓       | ✓            | ~140 MB |
| small  | 244 M      | ✓       | ✓            | ~460 MB |
| medium | 769 M      | ✓       | ✓            | ~1.5 GB |
| large  | 1550 M     |         | ✓            | ~3 GB   |

## Implementation Status

- [x] Core trait definition
- [x] Platform detection & factory
- [x] MLX adapter stub (macOS)
- [x] whisper.cpp adapter stub (Windows)
- [ ] MLX model integration
- [ ] whisper.cpp bindings
- [ ] Model download & caching
- [ ] Real-time streaming support
- [ ] GPU acceleration
- [ ] Tests

## Future Enhancements

1. **Streaming transcription** - Real-time word-by-word output
2. **Voice activity detection** - Automatic silence trimming
3. **Multi-language detection** - Auto-detect spoken language
4. **Custom vocabularies** - Improve accuracy for specific domains
5. **Sherpa-ONNX support** - Alternative backend for streaming
