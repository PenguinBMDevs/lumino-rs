# P0 功能实现需求文档

> 对标 Cubase / FL Studio / Reaper 功能基线，补齐 MIDI 编辑器的核心能力缺口。
> 完成以下 7 项 P0 功能后，Lumino 方可宣称"功能完备的 MIDI 编辑器"。

---

## 目录

1. [CC 控制器编辑器](#1-cc-控制器编辑器)
2. [音符分割与合并](#2-音符分割与合并)
3. [音轨混音器](#3-音轨混音器)
4. [移调](#4-移调)
5. [速度包络视图](#5-速度包络视图)
6. [弯音轮编辑器](#6-弯音轮编辑器)
7. [琶音器](#7-琶音器)

---

## 1. CC 控制器编辑器

### 现状

引擎层、解析层、导出层已完整支持 CC 事件（Controller Change）的读写和回放，但缺乏**可视化编辑界面**。用户当前只能通过加载含 CC 的 MIDI 文件并播放来间接验证，无法对 CC 曲线进行查看、绘制、修改。

### 需求描述

在钢琴卷帘下方新增 CC 控制器编辑面板，类似 Cubase 的 Controller Lane 或 FL Studio 的 CC 映射：

- 支持在多个 CC 控制器类型间切换（调制轮 CC1、音量 CC7、声像 CC10、表情 CC11、延音踏板 CC64 等）
- 支持在同一音轨的 CC 数据之间切换显示（一个面板 + 下拉选择器）
- 每个控制器的数据以**折线图/阶梯图**形式展示
- X 轴与钢琴卷帘的 tick 对齐并联动滚动
- Y 轴范围 0-127

### 交互操作

| 操作 | 行为 |
|------|------|
| 点击空白处 | 在该位置添加一个控制点，值与鼠标 Y 位置对应 |
| 拖拽已有控制点 | 修改该点的 tick 位置和值 |
| 双击控制点 | 删除该点 |
| 框选（Pointer 工具） | 选中多个控制点，可批量移动/删除 |
| 直线绘制 | 按住 Shift 点击两点间自动生成直线插值 |
| 清除全部 | 工具栏按钮一键清除当前 CC 类型的所有数据 |

### 数据模型

```rust
/// CC 控制点
#[derive(Debug, Clone)]
pub struct CcPoint {
    /// tick 位置
    pub tick: f32,
    /// 控制器值 (0-127)
    pub value: u8,
}

/// 音轨 CC 数据
#[derive(Debug, Clone, Default)]
pub struct CcData {
    /// 控制器编号 -> 控制点列表
    pub controllers: HashMap<u8, Vec<CcPoint>>,
}
```

### 技术要点

- X 轴坐标转换复用 `editor/coords.rs` 的 `tick_to_x` / `x_to_tick`
- 控制点渲染用 `iced_widget::canvas::Canvas`，折线用 `Stroke` + `Path`
- 数据存储在 `EditorData` 中，跟随 Undo/Redo 快照
- CC 事件在导出 MIDI / 音频时需一并写入
- 需要修改 `Track` 数据模型，增加 `CcData` 字段

### 优先级分析

CC 是 MIDI 协议的核心表达层。没有 CC 编辑器，用户无法创作表情、自动化控制、动态变化——MIDI 编辑器变成了"静态音符放置器"，失去了 MIDI 80% 的表达能力。

---

## 2. 音符分割与合并

### 现状

工具栏已定义 `Tool::Razor` 枚举变体（`crates/ui/src/toolbar/types.rs`），但实际分割逻辑**未实现**。右键菜单或快捷键也无 `Glue`（合并）操作。

### 需求描述

#### 2.1 音符分割 (Split / Razor Tool)

| 操作 | 行为 |
|------|------|
| 选择 Razor 工具后点击音符 | 在点击 tick 位置将音符一分为二 |
| 分割后左侧音符保持原 key，tick 不变，length = 点击点 - tick | |
| 分割后右侧音符保持原 key，tick = 点击点，length = 原末尾 - 点击点 | |
| 分割点吸附到当前 Snap 精度 | |
| 若分割点落在音符外部（未命中） | 无操作 |
| 命中判断 | 使用已有 `hit_test_note` 逻辑，HitType 不限 |

#### 2.2 音符合并 (Glue / Join)

| 操作 | 行为 |
|------|------|
| 选中多个同 key 的音符 → 快捷键/菜单执行合并 | 合并为单个音符 |
| 合并后音符 | tick = 最早音符的 tick |
| | length = 最晚音符末尾 - 最早音符的 tick |
| | velocity = 第一个音符的 velocity |
| | 中间的所有间隙被填充 |
| 若选中音符包含多个不同 key | 仅合并同 key 的相邻音符组 |
| 若选中音符中无同 key 音符 | 无操作 |

### 数据模型变更

无新增结构体。`Editor` 新增方法：

```rust
impl Editor {
    /// 在 tick 位置分割音符 index
    pub fn split_note(&mut self, index: usize, split_tick: f32) -> bool;

    /// 合并选中音符中同 key 的相邻音符
    pub fn glue_selected_notes(&mut self) -> usize;
}
```

### 技术要点

- `split_note` 需要先 `push_history()`，然后 remove 原音符、insert 两个新音符
- `glue_selected_notes` 需要对选中索引按 tick 排序，分组（同 key 且 tick 连续/重叠）
- 分割后需更新 `track_notes`、选中状态、悬停状态

---

## 3. 音轨混音器

### 现状

数据层已支持：
- `Track` 结构体有 `is_muted`、`solo` 字段（`crates/core/src/midi/track.rs`、`crates/ui/src/sidebar.rs`）
- 播放引擎已支持 `should_play(has_solo)` 逻辑
- `MidiView`/`MidiTrack` 有 `name`、`channel` 等属性

但**缺乏专用的混音器面板**。用户当前只能通过侧边栏的静音按钮做有限的开关控制，无法调整音量、声像，也看不到电平指示。

### 需求描述

新增混音器面板（Mixer View），可通过侧边栏 Route 或菜单打开：

| 功能 | 说明 |
|------|------|
| 音量推子 | 每个音轨一个垂直推子，范围 0-127（MIDI CC7），默认 100 |
| 声像旋钮 | 每个音轨一个声像控制 L63-C-R63（MIDI CC10），默认 64（center） |
| 静音按钮 | 复用现有 `is_muted` 逻辑 |
| Solo 按钮 | 复用现有 `solo` 逻辑 |
| 音轨名称 | 显示当前音轨名，可双击编辑 |
| 通道选择 | 下拉选择 MIDI 通道 (1-16) |
| 音量数值显示 | 推子旁显示当前数值 |
| 电平指示器 | 回放时显示实时 MIDI 活动指示（有 Note On 时闪烁） |
| 音轨颜色 | 显示音轨颜色标识 |

### 布局

```
┌────────────────────────────────────────────┐
│  混音器                                      │
├──────┬──────┬──────┬──────┬──────┬──────────┤
│ 音轨1 │ 音轨2 │ 音轨3 │ 音轨4 │ ...  │ 总输出   │
│ ┌──┐ │ ┌──┐ │ ┌──┐ │ ┌──┐ │      │ ┌──┐    │
│ │  │ │ │  │ │ │  │ │ │  │ │      │ │  │    │
│ │  │ │ │  │ │ │  │ │ │  │ │      │ │  │    │
│ │  │ │ │  │ │ │  │ │ │  │ │      │ │  │    │
│ └──┘ │ └──┘ │ └──┘ │ └──┘ │      │ └──┘    │
│ 100  │ 100  │ 100  │ 100  │      │ 100     │
│ [M][S]│ [M][S]│ [M][S]│ [M][S]│      │        │
│ 音轨1 │ 音轨2 │ 音轨3 │ 音轨4 │      │ Master │
├──────┴──────┴──────┴──────┴──────┴──────────┤
│  [M]=静音  [S]=Solo                         │
└────────────────────────────────────────────┘
```

### 数据模型

```rust
/// 混音器音轨状态
#[derive(Debug, Clone)]
pub struct MixerTrackState {
    /// 音量 (0-127, MIDI CC7)
    pub volume: u8,
    /// 声像 (0-127, MIDI CC10, 64=center)
    pub pan: u8,
    /// 是否展开（显示更多细节）
    pub expanded: bool,
}

/// 混音器状态
#[derive(Debug, Clone, Default)]
pub struct MixerState {
    pub tracks: Vec<MixerTrackState>,
    pub is_open: bool,
}
```

### 技术要点

- 新增 `crates/ui/src/mixer/` 模块
- 音量/声像通过 MIDI CC 事件在播放时实时发送（现有 `control_change` 接口已支持）
- 推子用 `iced_widget::vertical_slider`（需确认 iced 0.14 是否有，否则用 canvas 自绘）
- 电平指示器监听播放引擎的 Note On 事件，用瞬时计数器实现闪烁
- Master 推子控制全局音量
- 混音器设置保存到 Project 文件中

---

## 4. 移调

### 现状

完全缺失。无相关 gRPC 搜索命中。

### 需求描述

对选中音符按半音阶向上或向下移调：

| 操作 | 行为 |
|------|------|
| 选中音符 → 移调 +1 | 所有选中音符 key += 1 |
| 选中音符 → 移调 -1 | 所有选中音符 key -= 1 |
| 选中音符 → 移调 +12 | 高八度 |
| 选中音符 → 移调 -12 | 低八度 |
| 无选中音符 | 移调当前音轨**全部音符** |
| key 超出 0-255 范围 | clamp 到边界 |
| 快捷键 | `↑`/`↓` 半音，`Shift+↑`/`↓` 八度 |

### 数据模型变更

`Editor` 新增方法：

```rust
impl Editor {
    /// 按半音移调选中音符
    /// `semitones`: 正数=升高，负数=降低
    pub fn transpose_selected(&mut self, semitones: i16) -> usize;
}
```

### 技术要点

- 实现与 `note_flip.rs` 类似：push_history → 遍历 selected → 修改 key → clamp → mark_notes_changed
- 快捷键绑定在 `crates/ui/src/root/handlers.rs` 或 `editor_state/interaction.rs` 中
- 工具栏可加移调 +/-1 按钮

---

## 5. 速度包络视图

### 现状

- 全局 BPM 有常量 `DEFAULT_BPM`（`crates/core/src/midi/constants.rs`）
- `TempoChange` 类型存在（`crates/ui/src/playback/manager.rs`）
- 播放引擎支持 tempo 变更事件
- 但**缺乏可视化速度曲线编辑器**。用户只能通过代码或导入含速度变化的 MIDI 文件来间接使用。

### 需求描述

在钢琴卷帘的标尺区域上方或独立面板中，新增速度包络编辑功能：

| 功能 | 说明 |
|------|------|
| 显示 | 在 ruler 区域上方以折线图展示速度变化 |
| X 轴 | tick 位置，与钢琴卷帘联动 |
| Y 轴 | BPM 值 (20-999)，可缩放 |
| 添加速度点 | 在曲线上点击添加控制点 |
| 拖拽 | 修改控制点的 tick 位置和 BPM 值 |
| 删除 | 双击/右键删除控制点 |
| 默认 | 新工程在 tick=0 处有一个 120 BPM 控制点 |
| 播放头联动 | 播放时当前速度位置高亮显示 |
| 数值显示 | 悬停控制点时显示 BPM 精确值 |

### 数据模型

```rust
/// 速度控制点
#[derive(Debug, Clone)]
pub struct TempoPoint {
    /// tick 位置
    pub tick: f32,
    /// BPM 值
    pub bpm: f64,
}

/// 速度包络
#[derive(Debug, Clone)]
pub struct TempoEnvelope {
    /// 控制点列表，按 tick 升序
    pub points: Vec<TempoPoint>,
}

impl Default for TempoEnvelope {
    fn default() -> Self {
        Self {
            points: vec![TempoPoint { tick: 0.0, bpm: 120.0 }],
        }
    }
}
```

### 技术要点

- 渲染在 `grid/ruler.rs` 的 ruler 区域上方，与 ruler 共享 X 轴滚动
- 使用 `iced_widget::canvas::Canvas` + `Path` 绘制折线
- 控制点拖拽复用 `editor/drag.rs` 的拖拽框架
- 数据存储在 `MidiDocument` 级别（全局），非单音轨
- 导出 MIDI 时需将 TempoPoint 列表转换为 `MidiTempoEvent` 写入

---

## 6. 弯音轮编辑器

### 现状

- 引擎层有 `pitch_bend` 事件处理（`crates/ui/src/playback/engine/control.rs`）
- 解析层有 `pitch_bends: Vec<(f32, u8, f32)>`
- `OutputConnection` trait 有 `pitch_bend(&mut self, ch: u8, value: f32)`（`crates/midi/src/lib.rs`）
- 导出层支持弯音写入
- 但**缺乏可视化弯音曲线编辑器**

### 需求描述

弯音编辑作为 CC 控制面板的一个特殊类型（类似 CC 但值域不同）：

| 功能 | 说明 |
|------|------|
| 选择"Pitch Bend"作为控制器类型 | 显示弯音曲线 |
| Y 轴范围 | -8192 ~ +8191（14-bit 精度），显示为 -100% ~ +100% |
| 默认 | 弯音在 tick=0 处自动为 0（center） |
| 添加控制点 | 点击曲线添加，值以 14-bit 精度插值 |
| 拖拽 | 修改控制点位置和弯音值 |
| 删除 | 双击/右键删除 |
| 可视化 | 中心线（0 值）用虚线标出，正值区域绿色、负值区域红色 |
| 导出 | 弯音事件写入 MIDI/音频导出 |

### 数据模型

```rust
/// 弯音控制点
#[derive(Debug, Clone)]
pub struct PitchBendPoint {
    pub tick: f32,
    /// 弯音值，范围 -8192 ~ +8191
    pub value: i16,
}
```

### 技术要点

- 与 CC 编辑器共享同一个 Panel，在控制器下拉中增加 "Pitch Bend" 选项
- 14-bit 精度需注意 MIDI 协议中弯音是 2 字节（LSB + MSB），但 UI 层用 `i16` 简化
- 渲染用 `-8192` 到 `+8191` 映射到面板像素高度，中心线 `0` 在面板 50% 位置

---

## 7. 琶音器

### 现状

完全缺失。无相关代码或设计。

### 需求描述

内置琶音器（Arpeggiator），将输入的和弦/音符按指定模式自动生成琶音序列：

| 功能 | 说明 |
|------|------|
| 模式 | Up（上行）、Down（下行）、UpDown（上行后下行）、DownUp、Random（随机）、Chord（同时发音） |
| 速率 | 与当前网格精度对齐：全音符~64分音符 + 三连音 + 符点 |
| 范围 | 琶音跨越的八度数 (1-8) |
| 门限 (Gate) | 每个琶音音符的长度占原步进的比例 (25%-100%) |
| 力度变化 | 每步力度可按预设模式变化（固定/渐强/渐弱/随机） |
| 摆动 (Swing) | 每两步之间的时间偏移量 (0%-100%) |
| 触发方式 | **非破坏性**：琶音器生成的目标音符实时发送到 MIDI 输出，不写入钢琴卷帘 |
| 冻结 (Hold) | 松开 MIDI 键后继续琶音（直至再次按下停止） |
| 速率同步 | 琶音速率随工程 BPM 变化 |

### 架构设计

```
┌─────────────┐    ┌──────────────┐    ┌──────────────┐
│ MIDI输入     │───▶│ 琶音器引擎    │───▶│ MIDI输出      │
│ (键盘/音符)  │    │              │    │ (播放引擎)    │
└─────────────┘    │ · 模式       │    └──────────────┘
                   │ · 速率       │
                   │ · 门限       │
                   │ · 力度控制   │
                   │ · Swing      │
                   └──────────────┘
```

### 数据模型

```rust
/// 琶音器模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArpMode {
    Up,
    Down,
    UpDown,
    DownUp,
    Random,
    Chord,
}

/// 琶音器配置
#[derive(Debug, Clone)]
pub struct ArpeggiatorConfig {
    pub enabled: bool,
    pub mode: ArpMode,
    /// 步进间隔 tick 数（与网格精度对齐）
    pub step_ticks: f32,
    pub octave_range: u8,
    /// 门限比例 (0.0-1.0)
    pub gate: f32,
    /// Swing 比例 (0.0-1.0)
    pub swing: f32,
    pub hold: bool,
    /// 力度模式
    pub velocity_mode: ArpVelocityMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArpVelocityMode {
    Fixed(u8),
    Crescendo,
    Diminuendo,
    Random,
}
```

### 技术要点

- 琶音器引擎在 `crates/ui/src/playback/` 中实现，与 `PlaybackEngine` 配合
- 非破坏性设计：琶音音符只发往 MIDI 输出，不修改 `EditorData.notes`
- 触发时机：收到 Note On 时记录当前按下的 keys，按节拍时钟产生步进
- 步进时钟从播放引擎的 tick 位置驱动
- 琶音器 UI 配置面板：在工具栏或侧边栏新增琶音器设置区域
- MIDI 录制时琶音器输出**不写入录制结果**（与 DAW 行为一致）

---

## 验收标准

每个功能完成需通过以下 checkpoints：

| # | Checkpoint | 涉及功能 |
|---|-----------|---------|
| 1 | cargo build --release 通过 | 全部 |
| 2 | cargo clippy --all-targets 无新增 warning | 全部 |
| 3 | cargo test 全部通过 | 全部 |
| 4 | 单元测试覆盖核心逻辑分支 ≥ 80% | 全部 |
| 5 | UI 组件渲染交互正常（手动验证） | 全部 |
| 6 | 导出 MIDI 文件后在其他 DAW 中加载验证 | CC、弯音、速度包络 |
| 7 | 导入含 CC/弯音/速度变化的 MIDI 文件后编辑再导出，数据无损 | CC、弯音、速度包络 |

---

## 实施建议顺序

| 序号 | 功能 | 预估工时 | 理由 |
|------|------|---------|------|
| 1 | 移调 Transpose | 0.5d | 实现最简单，快速出活建立信心 |
| 2 | 音符分割/合并 Split & Glue | 1d | 基础编辑能力，Razor 工具已有入口 |
| 3 | 速度包络视图 Tempo Envelope | 2d | 引擎已支持 tempo 事件，只需补齐可视化层 |
| 4 | CC 控制器编辑器 | 3-4d | 核心表达层，但复杂度较高（数据模型+UI+导出） |
| 5 | 弯音轮编辑器 Pitch Bend | 1d | 与 CC 编辑器共享框架，增量实现 |
| 6 | 音轨混音器 Mixer | 2-3d | 新 UI 模块，但后端 mute/solo 已有 |
| 7 | 琶音器 Arpeggiator | 3-4d | 独立引擎 + UI，复杂度最高 |

**总计预估：13-17 人日**
