# Lumino 重构方案 —— 文档二：Crate 依赖关系重构

> 目标：将 26 个 crate 合并为 12 个，建立严格单向的依赖图

---

## 一、当前依赖问题诊断

### 1.1 依赖关系全景（问题版）

```
┌─────────────────────────────────────────────────────────────┐
│                        lumino-ui                              │
│  (依赖 17 个内部 crate + iced + wgpu + tokio + ...)          │
└──────────────┬──────────────────────────────────────────────┘
               │
    ┌──────────┼──────────┬──────────┬──────────┬──────────┐
    ▼          ▼          ▼          ▼          ▼          ▼
┌───────┐ ┌───────┐ ┌────────┐ ┌───────┐ ┌────────┐ ┌───────┐
│editor-│ │ midi- │ │ cloud  │ │collab-│ │ export │ │ dialog│
│state  │ │ io    │ │        │ │oration│ │        │ │       │
└───┬───┘ └───┬───┘ └───┬────┘ └───┬───┘ └───┬────┘ └───┬───┘
    │         │         │          │         │          │
    ▼         ▼         ▼          ▼         ▼          ▼
┌─────────────────────────────────────────────────────────────┐
│  core / event / message / extras / midi-model / note-core    │
│  (底层 crate 被上层穿透，message 反向依赖 midi-loader)       │
└─────────────────────────────────────────────────────────────┘
```

### 1.2 具体问题清单

| # | 问题 | 位置 | 影响 |
|---|------|------|------|
| 1 | `ui` 依赖 17 个内部 crate | `crates/editor/ui/Cargo.toml` | 编译 ui 时触发全量重编译 |
| 2 | `event` 反向依赖 `midi-loader` | `crates/core/event/Cargo.toml` | 基础设施依赖业务层 |
| 3 | `iced_*` feature 在 4 个 crate 不一致 | `ui`/`ui-core`/`ui-editor`/`ui-settings` | 实际编译的是 feature 最大公约数 |
| 4 | `workspace.dependencies` 65 项，30+ 单 crate 使用 | `Cargo.toml` | 依赖列表膨胀，版本管理困难 |
| 5 | `core` 目录 6 个 crate 互相引用 | `core/` 下各 crate | 核心层本应最稳定，却最混乱 |
| 6 | `gfx` 依赖 `editor-state` | `crates/editor/gfx/Cargo.toml` | 渲染层知道编辑器状态 |

### 1.3 循环依赖风险点

```
ui → editor-state → gfx → editor-state?  (目前 gfx 不直接依赖 editor-state，
                                           但 gfx 的 prepare 函数签名包含 EditorState)

message → midi-loader → midi-model → message?  (message 里定义了包含 MidiDocument 的事件)
```

---

## 二、Crate 合并方案

### 2.1 合并策略："功能内聚"原则

**合并规则**：
1. 同一功能域的 crate 合并（如所有音频相关）
2. 单向依赖的 crate 合并（如 `core` ← `event` ← `message`）
3. 频繁共同变更的 crate 合并（如 `editor-state` + `ui-editor`）

### 2.2 具体合并计划

#### Phase 1：核心层合并（4 → 1）

**合并**：`core` + `event` + `message` + `extras` → `lumino-core`

```rust
// lumino-core/src/lib.rs
pub mod config;      // 原 core/src/storage/config.rs
pub mod events;      // 原 event/src/
pub mod actions;     // 原 message/src/ 合并后的 Action enum
pub mod i18n;        // 原 extras/src/i18n/
pub mod types;       // 原 core/src/ 中的基础类型

// 重导出保持兼容（过渡期内）
pub use crate::events::*;
pub use crate::actions::*;
```

**关键变更**：
- `message` 中的 `MidiDocument` 引用改为 `Box<dyn Any>` 或泛型参数
- `event` 中的菜单事件不再依赖 `midi-loader`，改为延迟解析

#### Phase 2：音频层合并（6 → 1）

**合并**：`midi-io` + `midi-loader` + `midi-model` + `playback` + `note-core` + `midiplayer` → `lumino-audio`

```
lumino-audio/
├── src/
│   ├── lib.rs
│   ├── io/           # 原 midi-io/（含 KDMAPI FFI）
│   ├── loader/       # 原 midi-loader/
│   ├── model/        # 原 midi-model/（ChunkedList 等）
│   ├── playback/     # 原 playback/（Timeline, Manager）
│   ├── player/       # 原 midiplayer/（TextureWaterfall）
│   └── synth/        # 原 xsynth/（fork，保留独立目录）
```

**关键变更**：
- `midi-io` 的 KDMAPI unsafe 代码隔离到 `io/kdmapi.rs`
- `midi-model` 的 `ChunkedList` 作为公共 API 暴露
- `playback` 的 `Timeline` 不再直接依赖 `EditorData`，改为接收 `TempoMap`

#### Phase 3：编辑层合并（4 → 1）

**合并**：`editor-state` + `ui-editor` + `ui-settings` + `ui-core` → `lumino-editor`

```
lumino-editor/
├── src/
│   ├── lib.rs
│   ├── state/        # 原 editor-state/
│   ├── ui/           # 原 ui-editor/ + ui-core/
│   ├── settings/     # 原 ui-settings/
│   └── actions/      # 合并后的 Action 定义
```

**关键变更**：
- `ui-core` 的 Iced 组件和 `ui-editor` 的钢琴卷帘逻辑统一
- `editor-state` 的 `EditorData` 和 `ui-editor` 的交互逻辑统一
- `ui-settings` 的设置面板作为 `ui/` 的子模块

#### Phase 4：网络层合并（2 → 1）

**合并**：`cloud` + `collaboration` → `lumino-network`

```
lumino-network/
├── src/
│   ├── lib.rs
│   ├── cloud/        # 原 cloud/（FTP/SFTP/WebDAV）
│   ├── collaboration/# 原 collaboration/（实时协作）
│   └── protocol.rs   # 共享协议定义
```

**关键变更**：
- `cloud` 的 UI 面板逻辑移到 `lumino-editor`，`lumino-network` 只保留协议和传输
- `collaboration` 的 HTTP 覆盖层和 `cloud` 的传输层统一抽象

#### Phase 5：UI 入口瘦身（1 → 1，但职责清晰）

**保留**：`lumino-ui` → `lumino-app`

```
lumino-app/
├── src/
│   ├── lib.rs
│   ├── main.rs       # 入口
│   ├── window.rs     # 窗口管理
│   ├── event_loop.rs # 事件循环
│   └── app.rs        # App 结构体（持有 Editor + Gfx + Audio + Network）
```

**关键变更**：
- `host/` 和 `root/` 的区分取消，统一为 `App` 结构体
- `App` 不直接包含业务逻辑，只负责事件转发和生命周期管理

---

## 三、Workspace 依赖清理

### 3.1 当前问题

`Cargo.toml` 中 `[workspace.dependencies]` 有 65 项，其中大量只在 1 个 crate 中使用：

```toml
# 当前（问题示例）
[workspace.dependencies]
iced_renderer = "0.14"      # 只有 ui 用
iced_winit = "0.14"         # 只有 ui 用
iced_aw = "0.14"            # 只有 ui 用
ab_glyph = "0.2"            # 只有 export/video 用
flac-encoder = "0.6"        # 只有 audio 用
iso9660 = "0.1"             # 只有 extras 用
russh = "0.52"              # 只有 cloud 用
suppaftp = "6"              # 只有 cloud 用
reqwest_dav = "0.1"         # 只有 cloud 用
libloading = "0.8"          # 只有 midi-io 用
midir = "0.10"              # 只有 midi-io 用
```

### 3.2 清理后

```toml
# 新 workspace.dependencies（仅保留多 crate 共享的依赖）
[workspace.dependencies]
# 核心依赖（5+ crate 使用）
wgpu = "30"
iced = "0.14"
iced_core = "0.14"
iced_widget = "0.14"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["rt-multi-thread"] }
thiserror = "2"
anyhow = "1"

# 音频相关（3+ crate 使用）
cpal = "0.15"
midly = "0.5"

# 工具
bytemuck = { version = "1", features = ["derive"] }
```

**单 crate 使用的依赖**降级回各自 `Cargo.toml`：

```toml
# crates/lumino-app/Cargo.toml
[dependencies]
iced_renderer = "0.14"      # 只在 app 用
iced_winit = "0.14"         # 只在 app 用

# crates/lumino-audio/Cargo.toml
[dependencies]
flac-encoder = "0.6"        # 只在 audio 用
libloading = "0.8"          # 只在 audio 用（KDMAPI）
midir = "0.10"              # 只在 audio 用

# crates/lumino-network/Cargo.toml
[dependencies]
russh = "0.52"              # 只在 network 用
suppaftp = "6"              # 只在 network 用
reqwest_dav = "0.1"         # 只在 network 用
```

---

## 四、Feature 标志统一

### 4.1 当前问题

4 个 UI crate 各自声明 `iced_*` feature：

```toml
# crates/editor/ui/Cargo.toml
[dependencies]
iced_wgpu = { workspace = true, features = ["geometry"] }
iced_widget = { workspace = true, features = ["image"] }

# crates/editor/ui-core/Cargo.toml
[dependencies]
iced_wgpu = { workspace = true, features = ["geometry", "image"] }
iced_widget = { workspace = true, features = ["image"] }

# crates/editor/ui-editor/Cargo.toml
[dependencies]
iced_wgpu = { workspace = true, features = ["geometry", "image"] }
iced_widget = { workspace = true, features = ["image", "canvas"] }
```

**实际效果**：Cargo 会合并所有 feature，最终编译的是 `["geometry", "image", "canvas"]`，所谓的"精细控制"是幻觉。

### 4.2 统一方案

**原则**：`iced` 只出现在 `lumino-app` 和 `lumino-editor` 中，其他 crate 不直接接触 Iced。

```toml
# crates/lumino-app/Cargo.toml
[dependencies]
iced = { workspace = true, features = ["wgpu", "geometry", "image", "canvas"] }
iced_winit = { workspace = true }

# crates/lumino-editor/Cargo.toml
[dependencies]
iced_core = { workspace = true }      # 只需要核心类型（Color, Length, Point 等）
iced_widget = { workspace = true, features = ["image", "canvas"] }
# 不依赖 iced_wgpu（渲染在 lumino-gfx 中）

# crates/lumino-gfx/Cargo.toml
[dependencies]
wgpu = { workspace = true }
# 不依赖 iced（纯 wgpu 渲染）
```

---

## 五、依赖方向约束

### 5.1 新依赖图（严格单向）

```
                    ┌─────────────┐
                    │  lumino-app │  ← 唯一入口
                    └──────┬──────┘
           ┌───────────────┼───────────────┐
           ▼               ▼               ▼
    ┌────────────┐  ┌────────────┐  ┌────────────┐
    │lumino-editor│  │lumino-gfx  │  │lumino-audio│
    └──────┬─────┘  └─────┬──────┘  └─────┬──────┘
           │              │               │
           └──────────────┼───────────────┘
                          ▼
                   ┌────────────┐
                   │lumino-core │  ← 不依赖任何内部 crate
                   └────────────┘
                          ▲
           ┌──────────────┼───────────────┐
           │              │               │
    ┌──────┴─────┐  ┌─────┴──────┐  ┌─────┴──────┐
    │lumino-project│  │lumino-network│  │lumino-export│
    └────────────┘  └────────────┘  └────────────┘
```

### 5.2 禁止的依赖方向

| 禁止方向 | 原因 |
|---------|------|
| `core` → 任何内部 crate | 核心层必须最稳定 |
| `gfx` → `editor` | 渲染层不应知道编辑器存在 |
| `audio` → `editor` | 音频引擎应独立运行 |
| `network` → `editor` | 网络层通过事件与编辑器通信 |
| `project` → `editor` | 工程文件格式应独立 |
| `iced` 出现在 `gfx`/`audio`/`network` | UI 框架不应污染业务层 |
| `wgpu` 出现在 `editor`/`audio`/`network` | 渲染 API 应隔离 |

### 5.3 允许的跨层通信方式

```rust
// 方式 1：Trait 接口（推荐）
pub trait GfxContext {
    fn render(&mut self, view: &EditorView, document: &Document);
    fn resize(&mut self, width: u32, height: u32);
}

// 方式 2：事件流（用于异步/跨线程）
pub enum GfxEvent {
    NotesChanged { track: usize, range: Range<usize> },
    ViewportChanged { scroll: (f32, f32), zoom: (f32, f32) },
}

// 方式 3：回调（用于简单通知）
pub type OnPlaybackPosition = Box<dyn Fn(u32) + Send + Sync>;
```

---

## 六、实施步骤

### Step 1：准备阶段（1 周）

1. 创建新目录结构（`lumino-app/`, `lumino-editor/`, `lumino-audio/`, `lumino-core/`, `lumino-network/`）
2. 用 `pub use` 在新 crate 中重导出旧 crate 的 API（保持兼容）
3. 更新 `Cargo.toml` workspace 配置
4. 确保编译通过（此时新旧 crate 并存）

### Step 2：核心层迁移（1 周）

1. 将 `core/`、`event/`、`message/`、`extras/` 的代码物理移动到 `lumino-core/src/`
2. 解决 `message` 对 `midi-loader` 的反向依赖（用 `Box<dyn Any>` 或延迟解析）
3. 更新所有引用 `lumino_core::*`、`lumino_event::*`、`lumino_message::*` 的代码
4. 删除旧 crate 目录

### Step 3：音频层迁移（1 周）

1. 将 `midi-io/`、`midi-loader/`、`midi-model/`、`playback/`、`note-core/`、`midiplayer/` 移动到 `lumino-audio/src/`
2. 统一错误类型（目前各 crate 有自己的 `Error` enum）
3. 确保 KDMAPI FFI 代码隔离在 `io/kdmapi.rs`
4. 删除旧 crate 目录

### Step 4：编辑层迁移（2 周）

1. 将 `editor-state/`、`ui-editor/`、`ui-settings/`、`ui-core/` 移动到 `lumino-editor/src/`
2. 合并 `EditorData` 和 `ui-editor` 的状态管理
3. 统一 Iced feature 标志
4. 删除旧 crate 目录

### Step 5：网络层迁移（1 周）

1. 将 `cloud/`、`collaboration/` 移动到 `lumino-network/src/`
2. 将 UI 面板逻辑移回 `lumino-editor`
3. 统一传输层抽象
4. 删除旧 crate 目录

### Step 6：UI 入口重构（1 周）

1. 将 `ui/` 重构为 `lumino-app/`
2. 清理 `host/` 和 `root/` 的区分
3. 确保 `lumino-app` 只负责事件转发

### Step 7：清理与验证（1 周）

1. 运行全量测试
2. 检查编译时间变化
3. 检查二进制体积变化
4. 更新文档

**总预估时间**：8 周（2 个月）

---

## 七、验证清单

重构完成后，必须满足以下约束：

- [ ] `cargo tree | grep "lumino-"` 显示 12 个内部 crate
- [ ] `cargo check` 无循环依赖警告
- [ ] `lumino-core` 的 `Cargo.toml` 中无 `lumino-*` 依赖
- [ ] `lumino-gfx` 的 `Cargo.toml` 中无 `iced` 依赖
- [ ] `lumino-audio` 的 `Cargo.toml` 中无 `lumino-editor` 依赖
- [ ] `cargo build --release` 编译时间比重构前减少 ≥20%
- [ ] 所有 crate 至少有一个测试文件
