# Lumino-rs — **一个适用于完整 Black MIDI 工作流的高性能 MIDI 编辑器**

[中文]()  [英文]()

<p align="center">
  <img src="https://github.com/user-attachments/assets/fb93ee53-df7e-4d07-83f2-99a906703936" width="720" alt="主页" />
</p>

Lumino-rs 是一个使用 Rust 开发的新一代 MIDI 编辑器，基于现代化技术栈构建，提供低延迟、跨平台的 MIDI 编辑体验。项目在架构上优先保证渲染性能与交互响应速度，目标是在主流桌面平台上提供一致的现代化编辑体验。 [![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/PenguinBMDevs/lumino-rs)

## 特性

- **完整 Black MIDI 工作流** — 钢琴卷帘编辑器，工程走带（indev），视频导出，音频渲染，基础剪辑（indev）
- **高性能渲染** — 基于 wgpu 的 GPU 加速渲染，支持 CPU 渲染
- **现代化界面** — 基于 eugi 构建的UI体系，提供便捷快速的使用体验
- **多后端 MIDI I/O** — 支持 KDMAPI、Maestro、WinMM 及跨平台 xsynth 后端，提供强劲性能
- **丰富的主题系统** — 十余种主题丰富你的UI外观
- **跨平台使用** — 支持Windows、MacOS、Linux和Android（开发中）

## 效果图

| 模块名称 | 效果截图 |
| --- | --- |
| **主页** | <img src="https://github.com/user-attachments/assets/2e57f268-5ab6-4cc2-b531-44fa82dec8ab" width="720" alt="主页" /> |
| **钢琴卷帘** | <img src="https://github.com/user-attachments/assets/e9d67469-35cd-4779-bf58-b71537068a63" width="720" alt="钢琴卷帘1" /><br><br><img src="https://github.com/user-attachments/assets/a38b2c8a-e1bd-438e-9c23-ea6f24687f91" width="720" alt="钢琴卷帘2" /> |
| **工程走带** | <img src="https://github.com/user-attachments/assets/c883d689-708f-492f-b7ec-ed9bc14b56c8" width="720" alt="工程走带" /> |
| **播放器** | <img src="https://github.com/user-attachments/assets/f503471c-a713-4f64-9c43-bbad5a937bf7" width="720" alt="播放器" /> |
| **视频导出面板** | <img src="https://github.com/user-attachments/assets/95a2afc1-6cc6-4d18-bb76-5308e7ceee13" width="720" alt="视频导出面板" /> |
| **音频导出面板** | <img src="https://github.com/user-attachments/assets/524e0871-c1c6-4c4a-b92b-8e7129a62dd7" width="720" alt="音频导出面板" /> |

## 快速开始

需要 Rust 1.95.0 或更高版本（Windows 需 Visual Studio Build Tools，macOS 需 Xcode Command Line Tools，Linux 需 GCC/Clang 与 pkg-config）。

```bash
git clone https://github.com/PenguinBMDevs/lumino-rs.git
cd lumino-rs
cargo run              # 开发模式
cargo run --release    # 发布模式（优化）
```

> 首次运行会自动创建配置文件与日志目录（Windows：`%APPDATA%\PenguinBMDevs\lumino\config\`；macOS：`~/Library/Application Support/com.PenguinBMDevs.lumino/`；Linux：`~/.config/lumino/`），日志级别可通过 `RUST_LOG` 环境变量调整。

## 许可证

[AGPL-3.0](LICENSE)

## 贡献

欢迎提交 Issue 和 Pull Request。请阅读 [CONTRIBUTING.md](CONTRIBUTING.md) 了解开发规范。

## 致谢

### 贡献者
|名称|主页地址|
|-|-|
|btjawa|https://github.com/btjawa|
|Enderman-bm|https://github.com/Enderman-bm|
|IMT11D|https://github.com/IMT11D|
|Buickmeow|https://github.com/Buickmeow|

### 组件
|名称|简介|
|-|-|
|[winit](https://github.com/rust-windowing/winit)|窗口事件抽象|
|[wgpu](https://github.com/gfx-rs/wgpu)|跨平台 GPU 抽象|
|[egui](https://github.com/emilk/egui)|数据驱动的 UI 框架|
|[midir](https://github.com/Boddlnagg/midir)|Rust MIDI I/O 库|
|[Spiderweb](https://github.com/UnPrioritized/Spiderweb)|矢量绘图式 MIDI 音符画工具|
|[Comet](https://github.com/MrCHB1/Comet)|高性能 MIDI 渲染器|
|[xsynth](https://github.com/BlackMIDIDevs/xsynth)|跨平台 MIDI 合成器|
|[midly](https://github.com/kovaxis/midly)|纯 Rust MIDI 文件解析库|
