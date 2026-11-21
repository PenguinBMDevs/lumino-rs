# Lumino-rs — **A High-Performance MIDI Editor for the Complete Black MIDI Workflow**

[Chinese](https://github.com/PenguinBMDevs/lumino-rs/blob/dev/README-CN.md)  [English](https://github.com/PenguinBMDevs/lumino-rs/blob/dev/README.md)

<p align="center">
  <img src="https://github.com/user-attachments/assets/919985f6-3856-4da7-9349-f6647d7cc014" width="720" alt="Home" />
</p>

Lumino-rs is a new-generation MIDI editor developed in Rust. Built on a modern technology stack, it provides a low-latency, cross-platform MIDI editing experience. The project prioritizes rendering performance and interaction responsiveness in its architecture, aiming to provide a consistent modern editing experience on mainstream desktop platforms. [![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/PenguinBMDevs/lumino-rs)

## Features

- **Complete Black MIDI workflow** — piano roll editor, project transport (indev), video export, audio rendering, basic editing (indev)
- **High-performance rendering** — GPU-accelerated rendering based on wgpu, with CPU rendering support
- **Modern interface** — a UI system built on eugi, providing a convenient and fast user experience
- **Multi-backend MIDI I/O** — supports KDMAPI, Maestro, WinMM, and the cross-platform xsynth backend, delivering strong performance
- **Rich theme system** — more than ten themes enrich your UI appearance
- **Cross-platform** — supports Windows, macOS, Linux, and Android (in development)

## Screenshots

| Module Name | Screenshot |
| --- | --- |
| **Home** | <img src="https://github.com/user-attachments/assets/240463b4-9f03-4c89-a11a-e571b6744dcc" width="720" alt="Home" /> |
| **Piano Roll** | <img src="https://github.com/user-attachments/assets/a04a3f08-c85e-4c50-8ac6-dc87436c5bfe" width="720" alt="Piano Roll 1" /><br><br><img src="https://github.com/user-attachments/assets/badfbc5b-e05c-4487-8693-aad9727bd5e7" width="720" alt="Piano Roll 2" /> |
| **Project Transport** | <img src="https://github.com/user-attachments/assets/d2e6b3e1-dc38-47d4-9751-4c7d8b5d138d" width="720" alt="Project Transport" /> |
| **Player** | <img src="https://github.com/user-attachments/assets/f503471c-a713-4f64-9c43-bbad5a937bf7" width="720" alt="Player" /> |
| **Video Export Panel** | <img src="https://github.com/user-attachments/assets/eb8791fe-3c2c-4595-9bb9-238198c6159b" width="720" alt="Video Export Panel" /> |
| **Audio Export Panel** | <img src="https://github.com/user-attachments/assets/70193d5b-9902-4b4f-929b-8ad241d2dd8e" width="720" alt="Audio Export Panel" /> |

## Quick Start

Requires Rust 1.95.0 or later (Windows requires Visual Studio Build Tools, macOS requires Xcode Command Line Tools, Linux requires GCC/Clang and pkg-config).

```bash
git clone https://github.com/PenguinBMDevs/lumino-rs.git
cd lumino-rs
cargo run              # Development mode
cargo run --release    # Release mode (optimized)
```

> On first run, configuration files and log directories are automatically created (Windows: `%APPDATA%\PenguinBMDevs\lumino\config\`; macOS: `~/Library/Application Support/com.PenguinBMDevs.lumino/`; Linux: `~/.config/lumino/`). The log level can be adjusted via the `RUST_LOG` environment variable.

## License

[AGPL-3.0](LICENSE)

## Contributing

Issues and Pull Requests are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) for development guidelines.

## Acknowledgements

### Contributors
| Name | Homepage URL |
|-|-|
|btjawa|https://github.com/btjawa|
|Enderman-bm|https://github.com/Enderman-bm|
|IMT11D|https://github.com/IMT11D|
|Buickmeow|https://github.com/Buickmeow|

### Components
| Name | Description |
|-|-|
|[winit](https://github.com/rust-windowing/winit)|Window event abstraction|
|[wgpu](https://github.com/gfx-rs/wgpu)|Cross-platform GPU abstraction|
|[egui](https://github.com/emilk/egui)|Data-driven UI framework|
|[midir](https://github.com/Boddlnagg/midir)|Rust MIDI I/O library|
|[Spiderweb](https://github.com/UnPrioritized/Spiderweb)|Vector-drawing-style MIDI note painting tool|
|[Comet](https://github.com/MrCHB1/Comet)|High-performance MIDI renderer|
|[xsynth](https://github.com/BlackMIDIDevs/xsynth)|Cross-platform MIDI synthesizer|
|[midly](https://github.com/kovaxis/midly)|Pure Rust MIDI file parsing library|
