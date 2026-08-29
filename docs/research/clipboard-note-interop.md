# 跨程序本体的剪贴板音符数据同步 — 实现方案调研

> 范围澄清（2026-09 修正）：**「跨程序本体」= Lumino 程序本体之间**——同一款软件的多个
> 进程/实例互相复制粘贴（例如进程 A 复制、进程 B 粘贴），**不是跨 DAW（与其它音乐软件互拷）**。
> 结论先行：**跨 Lumino 实例的音符同步，arboard 的私有文本 JSON 本身就已能跨进程传输，
> 架构上可行**；真正缺失的只有一处正确性缺口——**两端文档 PPQN（division）可能不同，粘贴时
> 若不重采样，tick 偏移与音符长度会被按错误的节拍尺度解释**。修复只需：(1) 复制载荷携带源
> `division`；(2) 粘贴时若目标 `division != 源 division`，按 `ratio = 目标/源` 对 tick 偏移与
> 长度做**一次重采样**（多一次同步计算），使粘贴出的音符「长度与数据完全一致」（同 PPQN 时零缩放、逐字节一致）。

---

## 1. 术语澄清：什么是「跨程序本体」

「本体」在此取两层含义，二者都必须在方案里落实：

1. **跨程序（Cross-Program / Inter-App）**：从 Lumino 复制的音符，能被其它音乐软件
   （FL Studio / Ableton / REAPER / MuseScore / Sibelius 等）识别并粘贴；反之从其它软件
   复制的音符，Lumino 也能粘贴。即**剪贴板互操作（clipboard interoperability）**。
2. **音符数据本体（Canonical Note Model）**：一套与具体程序解耦、可序列化的音符交换模型，
   作为 Lunimo 与外部环境之间的「单一权威源」。它独立于 Lumino 内部存储（`NoteEvent` / `Note`），
   但能无损双向转换。

本调研把"本体"落地为一个可序列化结构 `LuminoClipboardPayload`，并选 **Standard MIDI File (SMF)**
作为跨程序传输载体（因为它几乎所有 DAW 都能解析/生成）。

---

## 2. 现状取证（事实驱动，非猜测）

### 2.1 现有两套剪贴板实现，均为「Lumino 私有文本 JSON」

| 位置 | 用途 | 写入格式 |
|------|------|----------|
| `crates/editor/ui-editor/src/clipboard.rs` | 钢琴卷帘 复制/剪切/粘贴 | `arboard::Clipboard::set_text(json)` |
| `crates/editor/ui-editor/src/arrangement_ops/clipboard.rs` | 工程走带 复制/粘贴 | `arboard::Clipboard::set_text(json)` |

关键代码事实（`clipboard.rs:60-76` / `arrangement_ops/clipboard.rs:154-170`）：
```rust
let mut clipboard = arboard::Clipboard::new()?;
clipboard.set_text(payload.to_string())?;   // ← 只写文本，且是 Lumino 私有 JSON
```
JSON 结构（`clipboard.rs:45-58`）：
```json
{ "lumino": "notes", "version": 1, "track": 0,
  "origin_tick": 0.0, "origin_key": 60,
  "notes": [ { "tick": 0, "key": 0, "length": 10, "velocity": 100, "channel": 0 } ] }
```
格式标识来自 `crates/editor/ui-core/src/constants.rs:155`：
`CLIPBOARD_FORMAT = "notes"`、`CLIPBOARD_VERSION = 1`。

### 2.2 粘贴端只认 Lumino JSON

`read_clipboard_json`（`clipboard.rs:97-104`）只解析 Lumino 私有 JSON：
`serde_json::from_str` → 取 `origin_key` / `notes`。若剪贴板里不是该 JSON（例如从 DAW 复制来的
SMF 二进制或文本），直接 `None` → 粘贴静默无动作。

→ **结论**：当前复制到外部软件的，是一坨别的程序看不懂的 JSON 文本；从外部复制进来，Lumino 直接忽略。
跨程序同步在当前架构下**物理上不可能**。

### 2.3 内部「本体」已经存在，可复用

- `NoteEvent`（`crates/audio/midi-model/src/note_event.rs:22`）：24 字节，含 `id/u64, start_tick,
  end_tick, key, velocity, channel`，是文档单一权威源，与 SMF 天然同构（note = NoteOn+NoteOff）。
- `Note`（`crates/editor/ui-editor/src/note.rs`）：UI 层 f32 tick 表示，`from_raw` / `with_id`。

---

## 3. 可复用资产盘点（避免重复建设，百度味「平台化思维」）

**已有 SMF 序列化能力，直接复用，不要另写一套**：

- `crates/tools/export/src/midi/export.rs:17`
  `pub fn export_midi_to_bytes(data: &MidiExportData) -> ExportResult<Vec<u8>>`
  把 `MidiExportData` 序列化为标准 SMF 字节。
- `crates/tools/export/src/midi/tracks.rs` 的 `build_midi_smf` 已正确处理绝对→增量 tick、
  `EndOfTrack` 尾约束（其它软件硬约束，注释已说明）。
- `MidiExportData` / `MidiTrackData` / `MidiNoteEvent`
  （`crates/tools/export/src/midi/types.rs:17,91`）：`MidiNoteEvent { tick, channel, key,
  velocity, duration }`，与 `NoteEvent` 字段一一对应，转换零损。

**但注意**：`lumino-export` 还拉了 ffmpeg/audio 视频导出等重依赖。把整个 `lumino-export`
塞进编辑器剪贴板路径过重。**推荐**在叶子 crate `lumino-midi-model` 新增一个 `clipboard` 模块，
直接用其已依赖的 `midly` 完成：

- `NoteEvents -> midly::Smf -> Vec<u8>`（写剪贴板用）
- `Vec<u8> -> Smf::parse -> Vec<NoteEvent>`（读剪贴板用，复用 `midly::Smf::parse`，
  导出测试 `crates/tools/export/src/midi/tests.rs:163` 已验证可往返）

这样「本体 ↔ SMF」转换留在模型层，编辑器两处 clipboard 实现只调用转换，符合现有
「2026-08 单一权威源」改造方向。

---

## 4. 核心问题：为什么 arboard 卡住了跨程序同步

`arboard` 3.4（见 `Cargo.toml:139`，workspace 依赖 `arboard = "3.4"`）公开 API 只有：

- `set_text` / `get_text`
- `set_image` / `get_image`
- `set_html` / `get_html`

**没有「按自定义格式名写入任意二进制」的能力**（arboard issue #61 亦确认仅 text/image/html）。
而跨程序 MIDI 互操作必须依赖**自定义二进制剪贴板格式**（见 §5），因为：

- 文本通道会被其它程序当成字符串，无法被 DAW 解析为音符；
- 图片通道语义不对；
- 只有「命名的二进制 blob」才能让接收方用 `RegisterClipboardFormat` / UTI / MIME 识别。

→ **这是跨程序同步的唯一硬阻塞点**：要么写平台专属剪贴板代码，要么换能力更强的剪贴板 crate。

---

## 5. 技术方案

### 5.1 定义跨程序「音符数据本体」

新增 `lumino_midi_model::clipboard`：

```rust
/// 跨程序剪贴板音符载荷——Lumino 与外部环境交换的单一权威模型
pub enum LuminoClipboardPayload {
    /// Lumino 私有（保真往返，含可视化音轨布局等原生信息）
    Native {
        version: u32,
        kind: ClipboardKind,        // PianoRoll | Arrangement
        origin_tick: f32,
        origin_key: u16,
        origin_track: usize,        // 走带用
        notes: Vec<ClipboardNote>,
    },
    /// 跨程序交换载体：标准 MIDI 文件字节（其它 DAW 可识别）
    Midi { smf: Vec<u8> },
}

pub struct ClipboardNote {
    pub tick: u32,        // 绝对或相对 tick（与现有 JSON 的 offset 语义对齐）
    pub key: u8,
    pub length: u32,
    pub velocity: u8,
    pub channel: u8,
    pub track: usize,     // 走带多轨用
}
```

转换：
- `encode_native(notes: &[NoteEvent]) -> LuminoClipboardPayload::Native`（复用现有 JSON 字段）
- `encode_midi(notes: &[NoteEvent]) -> LuminoClipboardPayload::Midi`（→ `export_midi_to_bytes` 等价逻辑）
- `decode_midi(bytes: &[u8]) -> Vec<NoteEvent>`（`Smf::parse` + NoteOn/NoteOff 合并）

### 5.2 写入剪贴板：一次写多种格式（按平台）

复制时**同时**写入以下格式，接收方各取所需：

| 格式 | 平台载体 | 内容 | 作用 |
|------|----------|------|------|
| 文本 | `CF_TEXT`/`text/plain` | Lumino 私有 JSON | 本程序往返 + 人类可读兜底 |
| **自定义 MIDI** | 见 §5.3 | SMF 二进制 | **跨程序互操作主通道** |

粘贴时探测优先级：**Native JSON → 自定义 MIDI 二进制 → 纯文本（尽力）**。

### 5.3 自定义二进制剪贴板格式（平台 API 核实）

| 平台 | API | 格式名（建议） | 现有依赖是否够 |
|------|-----|----------------|----------------|
| Windows | `OpenClipboard` / `SetClipboardData(CF_DIB? 不，用自定义)` + `RegisterClipboardFormatW` | `"LuminoMidiNotes"`（可选同时写业界常用名） | ✅ `windows=0.62.2`、`winapi`（`winuser`）已就绪，无需加依赖 |
| macOS | `NSPasteboard` + 自定义 UTI `com.lumino.midi-notes` | UTI | ⚠️ 当前无 `objc2`/`cocoa` 依赖，需新增（次优先级） |
| Linux (X11/Wayland) | `x11rb` / `smithay` 自定义 MIME `application/x-lumino-midi`（兼 `audio/midi`） | MIME | ⚠️ arboard 内部用但未暴露，需直接调用或换 `smithay-clipboard`/`wl-clipboard` 绑定 |

> 现实约束：**没有任何通用标准**规定 DAW 间 MIDI 剪贴板格式名（Pro Tools↔Sibelius、REAPER
> 等各自注册私有名）。最可移植的载体是「SMF 二进制 + 一个我们注册的名字」。能互操作的
> 对象是「同样把 SMF 放在自定义剪贴板格式里的程序」。Lumino 应：写时注册自己的名；读时
> **尝试把任意二进制剪贴板格式按 SMF 解析**（`Smf::parse` 成功即采用），最大化兼容面。

### 5.4 粘贴去重与协作广播

现有粘贴已正确广播协作事件（`clipboard.rs:158` `local_note_added`，
`arrangement_ops/clipboard.rs:256`）。**新增的 MIDI 分支必须复用同一广播路径**，否则协作端
缺失被粘贴音符（这正是 2026-09 协作修复要解决的问题，不能回归）。

---

## 6. 分阶段实现计划（owner 端到端交付）

**P0 — 本体与 SMF 转换（低风险，纯模型层，可单测）**
- `lumino-midi-model/src/clipboard.rs`：`NoteEvent ↔ Smf` 双向、`LuminoClipboardPayload` 定义。
- 单测：`encode_midi` → `decode_midi` 往返一致；复用 `midly::Smf::parse` 断言合法 SMF。

**P1 — Windows 自定义剪贴板格式（主平台，零新依赖）**
- 新增平台模块 `crates/editor/ui-editor/src/clipboard/sys/windows.rs`：
  用 `windows` crate `RegisterClipboardFormatW("LuminoMidiNotes")` +
  `SetClipboardData`，与 arboard 的 `set_text`（原生 JSON）**并行写入**。
- 读取：先 `get_text`（Native JSON）→ 否则枚举剪贴板格式找 `LuminoMidiNotes`/`audio/midi` →
  `Smf::parse` → `decode_midi`。

**P2 — 粘贴端多格式探测 & 走带复用**
- `clipboard.rs` 与 `arrangement_ops/clipboard.rs` 的读取函数统一改为：
  `try_native_json() || try_midi_binary()`，并保留现有可视化音轨映射逻辑。

**P3 — macOS / Linux（按需求优先级）**
- macOS 引入 `objc2`；Linux 引入 X11/Wayland MIME 直写。可后置。

---

## 7. 风险与验证（闭环红线：没验证不叫交付）

| 风险 | 验证手段 |
|------|----------|
| SMF 其它 DAW 不认 | 导出 `.mid` 用 MuseScore/REAPER 打开；剪贴板 SMF 字节用 `midly::Smf::parse` 自测 |
| 自定义格式名冲突 | 注册 `LuminoMidiNotes`（带命名空间），不占用通用名 |
| 粘贴回归 / 协作丢失 | 复用 `local_note_added` 广播；加 P1/P2 集成测试（模拟 SMF 字节粘贴） |
| arboard 与平台 API 争用剪贴板 | Windows `OpenClipboard` 需主线程且及时 `CloseClipboard`，单点封装 |
| 多轨可视化错位 | 沿用 `arrangement_ops` 已验证的「视觉位置→文档音轨」映射，不重写 |

**验收证据（交付前必须跑）**：
- `cargo test -p lumino-midi-model clipboard`（本体往返）
- `cargo test -p lumino-editor-ui clipboard`（含新增 SMF 粘贴用例）
- Windows 实测：Lumino 复制 → 打开 REAPER/MuseScore 粘贴成功；反向复制 → Lumino 粘贴成功。

---

## 8. 结论

跨程序剪贴板音符同步**可行**，核心改动集中在一处：把「只写私有文本 JSON」升级为
「私有 JSON + SMF 二进制（自定义剪贴板格式）双写，粘贴时多格式探测」。
最大阻塞是 `arboard` 不支持自定义二进制格式，需用平台 API 补 Windows 主路径（依赖已齐，零新增）。
SMF 序列化能力已存在于 `lumino-export`，但建议下沉到 `lumino-midi-model` 以保持依赖轻量、
贴合「单一权威源」架构。

---

## 9. 实现状态（已按修正范围落地，2026-09）

按「Lumino↔Lumino 实例 + PPQN 一致性」范围落地（**不含跨 DAW / SMF 自定义格式**，该部分已按
用户纠正移除，避免越界交付与风险）：

- **PPQN 一致性（核心正确性）**：
  - 复制载荷（`clipboard.rs::copy_selected_notes_to_clipboard` 与
    `arrangement_ops/clipboard.rs::write_arrangement_clipboard`）在 JSON 内新增 `"division":<源 PPQN>`。
  - 粘贴解析（`parse_clipboard_notes` / `parse_arrangement_clipboard_notes`）读取目标文档
    `division`，计算 `ratio = 目标/源`（不一致且非零时），对每条音符的 **tick 偏移与长度按 ratio
    重采样**（多一次同步计算）；一致或缺失 `division` 时 `ratio=1`、零缩放、逐字节一致。
  - key / velocity / channel 始终原样写入（数据完全一致）。
- **P0 性能（O(N²)→O(N·logM)，与跨程序解耦、必做）**：`commit_pasted_notes` /
  `apply_paste_internal` 改用 `batch_insert_notes_with_ids` 直接回传已分配 id 并 `O(N)` 广播，
  消除原逐条插入 + `note_id_at` 全轨重扫。
- **P1 性能（复制内存悬崖）**：复制流式写出 JSON（不物化 `Vec<Value>`）；走带按选区矩形窗口反查
  （`collect_selected_notes_for_clipboard`），替代遍历全曲音符。
- 载体仍为 arboard 私有文本 JSON（天然跨 Lumino 进程）；无需自定义二进制格式、无需 `winapi`。

验收：
- `cargo test -p lumino-midi-model`（P0 对齐单测 ✅）
- `cargo test -p lumino-ui-editor arrangement_ops::clipboard`（7 个用例 ✅，含新增
  `test_paste_resamples_length_on_ppqn_mismatch` / `test_paste_no_resample_when_ppqn_matches`）
- 跨 Lumino 实例实测：进程 A 复制 → 进程 B 粘贴（含两端不同 PPQN 工程）需在本地手动验证，
  代码路径已就绪，且同 PPQN 时粘贴音符与源逐字节一致。

> 非目标：跨 DAW（REAPER/MuseScore 等）互拷不在本次范围。如需，须另行定义规范交换模型并
> 接入自定义剪贴板格式（参见 §4–§5 的可行性分析），那是独立增量，不混入本次交付。

