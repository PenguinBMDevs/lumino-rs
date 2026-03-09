# Lumino RS

一个使用 Rust 开发的新一代 MIDI 编辑器，基于现代化技术栈构建，提供低延迟、跨平台的 MIDI 编辑体验。

## 特性

- 🎹 **专业 MIDI 编辑**：支持多轨编辑、音符绘制、力度编辑
- ⚡ **高性能渲染**：使用 wgpu 进行 GPU 加速渲染
- 🖥️ **原生体验**：基于 winit + iced，各平台原生外观
- 🔌 **双后端 MIDI I/O**：同时支持系统原生 API（Windows KDMAPI）和跨平台 midir 后端
- ⚙️ **配置持久化**：窗口状态、主题设置自动保存
- 📦 **工作区组织**：core/gfx/midi/ui 四个 crate 清晰解耦

## 技术栈

| 组件 | 技术 |
|------|------|
| 窗口/事件 | winit 0.30 |
| 渲染 | wgpu 0.27 |
| UI | iced 0.14 + iced_aw 0.13 |
| 配置 | toml 0.9, serde_json 1.0 |
| 日志 | tracing 0.1 |
| MIDI I/O | midir 0.10, libloading 0.9, windows 0.62 (Win) |
| 跨平台路径 | directories 6.0 |

## 构建

### 前置要求

- Rust 1.92.0 或更新
- 各平台构建工具：
  - **Windows**: Visual Studio Build Tools 或 MSVC
  - **macOS**: Xcode Command Line Tools
  - **Linux**: GCC/Clang, pkg-config

### 编译

```bash
# 克隆仓库
git clone https://github.com/BuickMeow/lumino-rs.git
cd lumino-rs

# 开发构建
cargo build

# 发布构建（优化）
cargo build --release
```

## 运行

```bash
# 开发模式
cargo run

# 发布模式
cargo run --release
```

首次运行会自动创建配置文件：
- **Windows**: `%APPDATA%\com.buickmeow.lumino\config.toml`
- **macOS**: `~/Library/Application Support/com.buickmeow.lumino/config.toml`
- **Linux**: `~/.config/com.buickmeow.lumino/config.toml`

配置文件包含 UI 主题等设置，窗口状态保存在同目录的 `ui_state.json`。

## 项目结构

```
lumino-rs/
├── crates/
│   ├── core/     # 核心抽象：事件系统、存储类型
│   ├── gfx/      # wgpu 渲染上下文封装
│   ├── midi/     # MIDI I/O 后端（libloading + midir）
│   └── ui/       # iced UI 应用层
├── src/
│   ├── main.rs          # 入口
│   ├── runner.rs        # 应用主循环
│   ├── storage.rs       # 配置/状态持久化
│   ├── logging.rs       # 日志初始化
│   └── platform/        # 平台特定初始化（macOS）
├── resources/icons/     # SVG 图标资源
└── .trae/skills/       # IDE 集成技能
```

## 开发

### 日志控制

通过 `RUST_LOG` 环境变量控制日志级别：

```bash
# 查看所有 lumino 日志（DEBUG 级别）
RUST_LOG=lumino=debug cargo run

# 仅显示 WARN 及以上
RUST_LOG=warn cargo run
```

### 代码检查

```bash
cargo clippy --all-targets --all-features
cargo fmt
```

## 许可证

[Mulan PSL v2](LICENSE)

## 贡献

欢迎提交 Issue 和 Pull Request。请阅读 [CONTRIBUTING.md](CONTRIBUTING.md) 了解开发规范。

## 致谢

- [winit](https://github.com/rust-windowing/winit) - 窗口事件抽象
- [wgpu](https://github.com/gfx-rs/wgpu) - 跨平台 GPU 抽象
- [iced](https://github.com/iced-rs/iced) - 数据驱动的 UI 框架
- [midir](https://github.com/Boddln/midir) - Rust MIDI I/O 库
