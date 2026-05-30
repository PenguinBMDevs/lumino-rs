# Lumino 项目文件结构设计

## 1. 设计目标

- 支持两种形态：**文件夹形式**（`.lmpj` 包文件夹）和 **单文件形式**（`.lmpj` 归档文件）
- 每个音轨独立存储为 `.lmtrack`，直接保存解析后的数据结构，加载时无需二次解析
- 导入的外部数据（MIDI/DMS/LMPJ）独立存储在 `data/loaded/` 下，保留解析后的二进制形式
- 所有数据文件均经过二进制压缩（bincode + zstd），兼顾速度与压缩比
- 作品元数据以 TOML 格式存储，兼顾人类可读与机器解析
- 图标等附属资源保持原始格式存入 `data/image/`

---

## 2. 形态一：文件夹形式（.lmpj 包文件夹）

文件夹形态的工程包以 `.lmpj` 为扩展名，本质上是一个按约定结构组织的目录，可直接浏览、版本控制（Git）和增量同步。

```
MyProject.lmpj/                       # 工程包文件夹（.lmpj 扩展名标识）
├── .lumino/                          # Lumino 内部文件（隐藏目录）
│   └── version                       # 工程文件格式版本号（纯文本，如 "1"）
├── metadata.toml                     # 作品元数据（TOML 格式）
├── data/
│   ├── project/                      # 作品内容数据
│   │   ├── tracks/                   # 音轨数据目录
│   │   │   ├── 000.lmtrack           # 音轨 0 的数据（CompactEvent 压缩存储）
│   │   │   ├── 001.lmtrack           # 音轨 1 的数据
│   │   │   ├── 002.lmtrack
│   │   │   └── ...
│   │   ├── tempo.lmtemp              # 全局速度变化数据（压缩存储）
│   │   ├── signature.lmsig           # 拍号/调号数据（压缩存储）
│   │   ├── controls.lmctl            # 控制事件数据（CC/PC/PB，压缩存储）
│   │   └── track_names.lmnames       # 音轨名称映射表（压缩存储）
│   ├── image/                        # 作品图标与图片资源
│   │   ├── icon.png                  # 工程图标（用户自定义）
│   │   ├── icon.svg
│   │   └── ...                       # 其他附属图片
│   └── loaded/                       # 导入的外部数据（解析后缓存）
│       ├── mid/                      # 导入的 MIDI 文件解析数据
│       │   ├── xxxxxxxx.lmloaded     # 压缩后的解析数据（bincode+zstd）
│       │   └── ...
│       ├── dms/                      # 导入的 DMS 文件解析数据
│       │   ├── xxxxxxxx.lmloaded
│       │   └── ...
│       └── lmpj/                     # 导入的其他 LMPJ 工程解析数据
│           ├── xxxxxxxx.lmloaded
│           └── ...
└── README.md                         # 可选：作品说明文件（用户可编辑）
```

---

## 3. 形态二：单文件形式（.lmpj 归档文件）

单文件形态是文件夹形态的打包集合体，使用自定义轻量级归档格式，保持与文件夹形态完全等价的数据内容。

### 3.1 归档格式结构

```
+----------------------------------+
|         LMPJ 归档文件             |
+==================================+
| 文件头 (Header)                  |
|   - 魔数: b"LMPJ" (4 bytes)      |
|   - 格式版本: u16 (当前为 1)      |
|   - 压缩标志: u8                  |
|   - 文件表偏移: u64               |
|   - 文件表压缩后大小: u64         |
|   - 文件表原始大小: u64           |
|   - 创建时间戳: u64 (unix_secs)   |
|   - 保留字段: 16 bytes            |
+----------------------------------+
|                                  |
| 数据区（各文件内容顺序存储）      |
|                                  |
|  [metadata.toml.zst]             |
|  [.lumino/version.zst]           |
|  [data/project/tracks/000.lm...] |
|  [data/project/tracks/001.lm...] |
|  [data/project/tempo.lmtemp.zst] |
|  [data/image/icon.png.zst]       |
|  [data/loaded/mid/...]           |
|  ...                             |
|                                  |
+----------------------------------+
| 文件表（zstd 压缩后的 FileTable） |
|   - 条目数量: u32                 |
|   - 条目列表: Vec<FileEntry>      |
|                                  |
| FileEntry:                       |
|   - 路径长度: u16                 |
|   - 路径字符串: UTF-8             |
|   - 数据偏移: u64                 |
|   - 压缩后大小: u64               |
|   - 原始大小: u64                 |
|   - CRC32 校验: u32               |
|   - 是否压缩: u8                  |
+----------------------------------+
```

### 3.2 归档格式特性

| 特性 | 说明 |
|------|------|
| 随机访问 | 通过文件表可 O(1) 定位任意文件，无需全量解压 |
| 流式读取 | 支持边下载边解析，适合网络传输场景 |
| CRC32 校验 | 每个文件独立校验，可检测数据损坏 |
| 可选压缩 | 文件表中记录是否压缩，已压缩数据（如 PNG）可标记为不压缩 |
| 增量更新 | 重写时只需替换变更的数据块和文件表 |

---

## 4. 文件格式详细规范

### 4.1 metadata.toml — 作品元数据

```toml
# Lumino Project Metadata
# 格式版本，用于未来兼容
format_version = 1

[project]
# 作品名称
name = "Untitled Project"
# 作者
author = "Anonymous"
# 创建时间（ISO 8601）
created_at = "2026-05-28T10:30:00+08:00"
# 最后修改时间
modified_at = "2026-05-28T14:22:00+08:00"
# 作品描述
description = "A short description of this project."
# 使用的 lumino 版本
lumino_version = "0.1.0"

[audio]
# PPQN (每四分音符脉冲数)
division = 480
# 总 tick 数
total_ticks = 460800
# 音轨数量
track_count = 16
# 总音符数
total_notes = 1523400
# 默认 BPM
default_bpm = 120.0

[tracks]
# 音轨元数据数组（按 track_id 顺序）
[[tracks.entries]]
track_id = 0
name = "Piano Right"
channel = 0
visibility = "visible"    # visible / muted / hidden
solo = false
note_count = 450000

[[tracks.entries]]
track_id = 1
name = "Piano Left"
channel = 1
visibility = "visible"
solo = false
note_count = 380000

# ... 更多音轨

[loaded]
# 导入的外部文件清单
[[loaded.files]]
# 导入文件的唯一标识（SHA-256 前 8 字节 hex）
id = "a1b2c3d4"
# 原始文件名
original_name = "original.mid"
# 文件类型: "mid" / "dms" / "lmpj"
format = "mid"
# 导入时间
imported_at = "2026-05-28T11:00:00+08:00"
# 对应存储路径（相对于 data/loaded/）
storage_path = "mid/a1b2c3d4.lmloaded"
# 原始文件信息（可选）
original_info = { track_count = 16, total_notes = 1000000, division = 480 }

[settings]
# 工程级设置（覆盖全局配置）
theme = "Dark"
# 音源后端: "xsynth" / "kdmapi" / "system"
synth_backend = "xsynth"
# 音色库路径（相对或绝对）
soundfont_path = ""
# 自动滚动模式: "fixed_indicator" / "scrolling" / "off"
auto_scroll_mode = "scrolling"
# 是否启用 256 键扩展键盘
enable_256key = false
# 速度过滤阈值
velocity_filter_threshold = 1

[stats]
# 统计信息（只读，由 lumino 自动维护）
edit_count = 1523
playback_count = 45
export_count = 3
working_time_seconds = 3600
```

### 4.2 .lmtrack — 音轨数据文件

每个 `.lmtrack` 文件存储单个音轨的解析后事件数据，采用 **bincode 序列化 + zstd 压缩**。

#### 数据结构（Rust）

```rust
/// 音轨数据文件头（8 bytes）
#[derive(Debug, Clone)]
pub struct LmtrackHeader {
    /// 魔数: b"LMTR" (4 bytes)
    pub magic: [u8; 4],
    /// 音轨数据格式版本: u16
    pub version: u16,
    /// 音轨编号: u16
    pub track_id: u16,
}

/// 音轨事件存储结构（序列化主体）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LmtrackData {
    /// 音轨元数据
    pub meta: TrackMeta,
    /// 事件数据（CompactEvent 数组的原始字节）
    /// 注意：事件已按 tick 排序，直接写入无需再处理
    pub events: Vec<u8>,          // CompactEvent[] 的扁平字节数组
    /// 事件数量（用于解码时验证）
    pub event_count: u64,
    /// 此音轨的音符数量（NoteOn 事件数）
    pub note_count: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrackMeta {
    /// 音轨编号
    pub track_id: u16,
    /// 音轨名称
    pub name: String,
    /// MIDI 通道 (0-15)
    pub channel: u8,
    /// 端口 (0-15)
    pub port: u8,
    /// 可见性
    pub visibility: TrackVisibilitySer,
    /// Solo 状态
    pub solo: bool,
    /// 是否为鼓音轨
    pub is_drum: bool,
    /// 总 tick 范围（此音轨最后一个事件的 tick）
    pub max_tick: u32,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum TrackVisibilitySer {
    Visible = 0,
    Muted = 1,
    Hidden = 2,
}
```

#### 二进制布局

```
+------------------------------------------------+
| .lmtrack 文件                                   |
+================================================+
| 文件头 (8 bytes)                                |
|   magic      : [u8; 4] = b"LMTR"               |
|   version    : u16   = 1                       |
|   track_id   : u16                            |
+------------------------------------------------+
| zstd 压缩数据区                                 |
|   (LmtrackData 的 bincode 编码后压缩)           |
|                                                |
|   LmtrackData {                                |
|     meta: TrackMeta { ... },                   |
|     events: Vec<u8>  // CompactEvent 扁平数组   |
|                      // len = event_count * 12  |
|     event_count: u64,                          |
|     note_count: u64,                           |
|   }                                            |
+------------------------------------------------+
```

#### 事件数据说明

- `events` 字段存储的是 `CompactEvent` 数组的扁平字节表示
- 每个 `CompactEvent` 固定 **12 字节**（详见 `lumino_midi::compact::CompactEvent`）
- 事件已按 tick 排序，加载时直接映射到内存即可使用
- 无需二次解析，直接从文件加载到 `Vec<CompactEvent>`

### 4.3 .lmtemp — 全局速度变化数据

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LmtempData {
    /// 速度变化列表: (tick, bpm)
    pub tempo_changes: Vec<(u32, f32)>,
    /// 默认 BPM（如果列表为空则使用此值）
    pub default_bpm: f32,
}
```

- 文件头魔数: `b"LMTM"`
- 主体使用 bincode + zstd 压缩

### 4.4 .lmsig — 拍号/调号数据

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LmsigData {
    /// 拍号变化: (tick, numerator, denominator)
    pub time_signatures: Vec<(u32, u8, u8)>,
    /// 调号变化: (tick, key, is_major)
    pub key_signatures: Vec<(u32, i8, bool)>,
}
```

- 文件头魔数: `b"LMSG"`
- 主体使用 bincode + zstd 压缩

### 4.5 .lmctl — 控制事件数据

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LmctlData {
    /// 控制变更事件: (tick, track_id, channel, controller, value)
    pub control_changes: Vec<(u32, u16, u8, u8, u8)>,
    /// 程序变更事件: (tick, track_id, channel, program)
    pub program_changes: Vec<(u32, u16, u8, u8)>,
    /// 弯音事件: (tick, track_id, channel, value)
    pub pitch_bends: Vec<(u32, u16, u8, i16)>,
}
```

- 文件头魔数: `b"LMCT"`
- 主体使用 bincode + zstd 压缩

### 4.6 .lmnames — 音轨名称映射表

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LmnamesData {
    /// 音轨名称列表: index = track_id
    pub track_names: Vec<Option<String>>,
}
```

- 文件头魔数: `b"LMNM"`
- 主体使用 bincode + zstd 压缩
- 此文件为冗余存储（各 .lmtrack 也包含 name），用于快速浏览时无需加载完整音轨

### 4.7 .lmloaded — 导入的外部数据缓存

`data/loaded/{mid,dms,lmpj}/` 目录下存储的是从外部导入文件的解析后数据快照。

文件名格式: `{8位hex哈希}.lmloaded`

哈希计算: 对原始文件的 SHA-256 取前 8 字节转 hex，用于唯一标识和去重。

#### MIDI 导入数据（.lmloaded in mid/）

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LoadedMidiData {
    /// 原始文件信息
    pub original_info: MidiInfo,
    /// 原始 MIDI 字节数据（保留原始数据以便导出）
    pub raw_midi_bytes: Vec<u8>,
    /// 解析后的文档（可选，懒加载）
    /// 如果已解析，存储 CompactEvent 扁平数组
    pub parsed_events: Option<Vec<u8>>,
    /// 解析后的音轨范围
    pub track_event_ranges: Option<Vec<(usize, usize)>>,
    /// 解析后的 tempo 变化
    pub tempo_changes: Option<Vec<(u32, f32)>>,
    /// 导入时间
    pub imported_at: String,  // ISO 8601
}
```

#### DMS 导入数据（.lmloaded in dms/）

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LoadedDmsData {
    /// 原始 DMS 信息
    pub original_info: DmsInfo,
    /// 轻量级 DMS 原始数据（解压后）
    pub raw_dms_data: Vec<u8>,
    /// 是否已转换为 MIDI
    pub converted_to_midi: bool,
    /// 转换后的 MIDI 字节（如果已转换）
    pub converted_midi_bytes: Option<Vec<u8>>,
    /// 导入时间
    pub imported_at: String,
}
```

#### LMPJ 导入数据（.lmloaded in lmpj/）

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LoadedLmpjData {
    /// 原始 LMPJ 信息
    pub original_info: MidiInfo,
    /// 原始 LMPJ 的 MidiInfo
    pub midi_info: MidiInfo,
    /// 原始 MIDI 字节数据
    pub midi_data: Vec<u8>,
    /// 导入时间
    pub imported_at: String,
}
```

所有 `.lmloaded` 文件:
- 文件头魔数: `b"LMLD"`
- 主体使用 bincode + zstd 压缩

---

## 5. 与现有代码的映射关系

### 5.1 核心类型映射

| 新格式组件 | 对应现有代码 | 说明 |
|-----------|-------------|------|
| `metadata.toml` | 新增 | 当前 `LmpjData` 没有元数据，需要新增 |
| `.lmtrack` 文件 | `lumino_midi::compact::CompactEvent` | 现有 12 字节紧凑格式直接使用 |
| `LmtrackData.meta` | `TrackView` + 扩展 | 基于现有 `TrackManager` 中的 `TrackView` |
| `.lmtemp` | `MidiDocument.tempo_changes` | 直接提取 |
| `.lmsig` | `MidiDocument` 中的 meta 事件 | 提取拍号/调号 |
| `.lmctl` | `MidiDocument.control_events` | 提取控制事件 |
| `.lmnames` | `MidiDocument.track_names` | 直接提取 |
| `.lmloaded` (mid) | `ParsedMidi` | 现有解析后数据的序列化形式 |
| `.lmloaded` (dms) | `ParsedDms` | 现有解析后数据的序列化形式 |
| `.lmloaded` (lmpj) | `LmpjData` | 现有 LMPJ 数据的提取形式 |

### 5.2 加载流程映射

```
现有流程:
  .lmpj 文件 -> decode_lmpj() -> LmpjData { info, midi_data }
                           -> to_parsed_midi() -> ParsedMidi { info, midi_data, document: None }
                           -> 需要时重新解析 midi_data -> MidiDocument

新流程（文件夹形态）:
  .lmpj 文件夹 -> 读取 metadata.toml -> 工程元数据
              -> 读取 .lmtrack 文件 -> 每个音轨直接得到 CompactEvent[]（无需解析）
              -> 读取 .lmtemp -> tempo_changes
              -> 读取 .lmsig -> time/key signatures
              -> 读取 .lmctl -> control events
              -> 组合成 MidiDocument（内存中拼接 events 和索引）

新流程（单文件形态）:
  .lmpj 文件 -> 解析归档文件头 -> 读取文件表
            -> 按需解压各文件 -> 同上流程
```

### 5.3 保存流程映射

```
现有流程:
  ParsedMidi -> LmpjData::from_parsed_midi() -> encode_lmpj() -> .lmpj 文件
  （仅保存 info + 原始 midi_data）

新流程（文件夹形态）:
  EditorData + MidiDocument -> 写入 metadata.toml
                            -> 按音轨拆分 -> 各 .lmtrack 文件
                            -> 提取 tempo -> .lmtemp
                            -> 提取 signature -> .lmsig
                            -> 提取 controls -> .lmctl
                            -> 提取 track_names -> .lmnames
                            -> 保留 loaded/ 中的已有数据

新流程（单文件形态）:
  同上，最后将文件夹内容打包为 .lmpj 归档文件
```

---

## 6. 向后兼容性

### 6.1 旧版 LMPJ 文件支持

旧版 `.lmpj` 文件（单文件 bincode+zstd 格式）继续支持读取：

```rust
// 在 loader 中检测文件类型
pub async fn load_project(path: PathBuf) -> Result<Project> {
    if path.is_dir() {
        // 新格式：文件夹形态
        load_project_folder(path).await
    } else {
        // 检测是旧版 LMPJ 还是新版归档
        let bytes = tokio::fs::read(&path).await?;
        if &bytes[0..4] == b"LMPJ" {
            // 新版：单文件归档形态
            load_project_archive(&bytes).await
        } else {
            // 旧版：直接 decode_lmpj
            load_legacy_lmpj(&bytes).await
        }
    }
}
```

### 6.2 导入路径

- **打开旧版 .lmpj** -> 自动转换为新格式（提示保存）
- **打开 .mid/.midi** -> 解析后存储到 `data/loaded/mid/`，同时在 `data/project/` 创建音轨
- **打开 .dms** -> 解析后存储到 `data/loaded/dms/`，转换后在 `data/project/` 创建音轨
- **打开其他 .lmpj 作为引用** -> 解析后存储到 `data/loaded/lmpj/`

---

## 7. Rust 实现草案

### 7.1 核心类型定义

```rust
// crates/core/src/project/mod.rs

pub mod metadata;
pub mod track;
pub mod archive;
pub mod folder;

use metadata::ProjectMetadata;
use track::{LmtrackData, LmtrackHeader};

/// 工程数据（内存中表示）
#[derive(Debug)]
pub struct LuminoProject {
    /// 元数据
    pub metadata: ProjectMetadata,
    /// 音轨数据（懒加载：未修改的音轨可保持磁盘映射）
    pub tracks: Vec<TrackSlot>,
    /// 全局速度变化
    pub tempo_changes: Vec<(u32, f32)>,
    /// 拍号变化
    pub time_signatures: Vec<(u32, u8, u8)>,
    /// 调号变化
    pub key_signatures: Vec<(u32, i8, bool)>,
    /// 控制事件
    pub control_changes: Vec<(u32, u16, u8, u8, u8)>,
    /// 程序变更
    pub program_changes: Vec<(u32, u16, u8, u8)>,
    /// 导入的外部文件
    pub loaded_files: Vec<LoadedFileEntry>,
}

/// 音轨槽（支持懒加载）
#[derive(Debug)]
pub enum TrackSlot {
    /// 未加载（仅在文件中有数据）
    Unloaded { track_id: u16, path: std::path::PathBuf },
    /// 已加载到内存
    Loaded(LmtrackData),
    /// 已修改（需要保存）
    Modified(LmtrackData),
}

/// 导入文件条目
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LoadedFileEntry {
    pub id: String,
    pub original_name: String,
    pub format: LoadedFormat,
    pub imported_at: String,
    pub storage_path: std::path::PathBuf,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoadedFormat {
    Mid,
    Dms,
    Lmpj,
}
```

### 7.2 序列化/反序列化

```rust
// crates/core/src/project/track.rs

use lumino_midi::compact::CompactEvent;

impl LmtrackData {
    /// 编码为字节（bincode + zstd）
    pub fn encode(&self) -> Result<Vec<u8>, crate::CoreError> {
        let mut result = Vec::new();
        
        // 写入文件头
        let header = LmtrackHeader {
            magic: *b"LMTR",
            version: 1,
            track_id: self.meta.track_id,
        };
        result.extend_from_slice(&header.magic);
        result.extend_from_slice(&header.version.to_le_bytes());
        result.extend_from_slice(&header.track_id.to_le_bytes());
        
        // bincode 序列化主体
        let serialized = bincode::serialize(self)
            .map_err(|e| crate::CoreError::Encoding(format!("lmtrack bincode: {e}")))?;
        
        // zstd 压缩
        let compressed = zstd::stream::encode_all(
            std::io::Cursor::new(serialized), 3
        ).map_err(|e| crate::CoreError::Compression(format!("lmtrack zstd: {e}")))?;
        
        result.extend_from_slice(&compressed);
        Ok(result)
    }
    
    /// 从字节解码
    pub fn decode(bytes: &[u8]) -> Result<Self, crate::CoreError> {
        if bytes.len() < 8 {
            return Err(crate::CoreError::FileFormat("lmtrack: too short".into()));
        }
        
        // 验证魔数
        if &bytes[0..4] != b"LMTR" {
            return Err(crate::CoreError::FileFormat("lmtrack: invalid magic".into()));
        }
        
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != 1 {
            return Err(crate::CoreError::FileFormat(
                format!("lmtrack: unsupported version {version}")
            ));
        }
        
        // zstd 解压
        let decompressed = zstd::stream::decode_all(
            std::io::Cursor::new(&bytes[8..])
        ).map_err(|e| crate::CoreError::Compression(format!("lmtrack decompression: {e}")))?;
        
        // bincode 反序列化
        bincode::deserialize(&decompressed)
            .map_err(|e| crate::CoreError::Encoding(format!("lmtrack decode: {e}")))
    }
    
    /// 从 CompactEvent 切片创建
    pub fn from_compact_events(
        meta: TrackMeta,
        events: &[CompactEvent],
    ) -> Self {
        let note_count = events.iter()
            .filter(|e| matches!(e.kind(), lumino_midi::compact::EventKind::NoteOn))
            .count() as u64;
        
        // CompactEvent 扁平化为字节数组
        let mut event_bytes = Vec::with_capacity(events.len() * 12);
        for ev in events {
            event_bytes.extend_from_slice(ev.as_bytes());
        }
        
        Self {
            meta,
            events: event_bytes,
            event_count: events.len() as u64,
            note_count,
        }
    }
    
    /// 获取 CompactEvent 迭代器（零拷贝视图）
    pub fn compact_events(&self) -> impl Iterator<Item = CompactEvent> + '_ {
        self.events.chunks_exact(12)
            .map(|chunk| {
                let bytes: &[u8; 12] = chunk.try_into().expect("12 bytes");
                CompactEvent::from_bytes(bytes)
            })
    }
}
```

### 7.3 归档读写

```rust
// crates/core/src/project/archive.rs

/// LMPJ 归档文件头
#[derive(Debug, Clone, Copy)]
pub struct ArchiveHeader {
    pub magic: [u8; 4],           // b"LMPJ"
    pub version: u16,             // 1
    pub compression_flags: u8,    // 0x01 = zstd
    pub file_table_offset: u64,
    pub file_table_compressed_size: u64,
    pub file_table_original_size: u64,
    pub created_at: u64,          // unix timestamp
    pub _reserved: [u8; 16],
}

impl ArchiveHeader {
    pub const SIZE: usize = 4 + 2 + 1 + 8 + 8 + 8 + 8 + 16; // 55 bytes
}

/// 归档文件表条目
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: String,
    pub data_offset: u64,
    pub compressed_size: u64,
    pub original_size: u64,
    pub crc32: u32,
    pub is_compressed: bool,
}

/// 读取归档中的指定文件
pub fn read_file_from_archive(
    archive_bytes: &[u8],
    file_path: &str,
) -> Result<Option<Vec<u8>>, crate::CoreError> {
    let header = read_archive_header(archive_bytes)?;
    let file_table = read_file_table(archive_bytes, &header)?;
    
    let entry = file_table.iter().find(|e| e.path == file_path);
    match entry {
        Some(e) => {
            let data = &archive_bytes[e.data_offset as usize..
                (e.data_offset + e.compressed_size) as usize];
            
            if e.is_compressed {
                let decompressed = zstd::stream::decode_all(std::io::Cursor::new(data))
                    .map_err(crate::CoreError::Compression)?;
                Ok(Some(decompressed))
            } else {
                Ok(Some(data.to_vec()))
            }
        }
        None => Ok(None),
    }
}
```

---

## 8. 文件格式速查表

| 扩展名 | 魔数 | 内容 | 压缩方式 |
|--------|------|------|----------|
| `.lmpj` (文件夹) | - | 目录结构 | - |
| `.lmpj` (单文件) | `LMPJ` | 归档文件 | 文件级 zstd |
| `.lmtrack` | `LMTR` | 单音轨事件数据 | bincode + zstd |
| `.lmtemp` | `LMTM` | 全局速度变化 | bincode + zstd |
| `.lmsig` | `LMSG` | 拍号/调号 | bincode + zstd |
| `.lmctl` | `LMCT` | 控制事件 | bincode + zstd |
| `.lmnames` | `LMNM` | 音轨名称映射 | bincode + zstd |
| `.lmloaded` | `LMLD` | 导入数据缓存 | bincode + zstd |
| `metadata.toml` | - | 工程元数据 | zstd (归档内) |

---

## 9. 与 lumino-rs 现有 crate 的集成点

```
lumino-core
├── src/
│   ├── midi/
│   │   ├── document.rs          # MidiDocument 可导出为 .lmtrack 集合
│   │   ├── track.rs             # TrackManager -> TrackMeta 映射
│   │   └── info.rs              # MidiInfo -> metadata.toml 映射
│   └── project/                 # 新增模块
│       ├── mod.rs               # LuminoProject, TrackSlot
│       ├── metadata.rs          # ProjectMetadata, metadata.toml 读写
│       ├── track.rs             # LmtrackData, LmtrackHeader 编解码
│       ├── archive.rs           # .lmpj 归档格式读写
│       ├── folder.rs            # 文件夹形态读写
│       └── loaded.rs            # .lmloaded 格式定义

lumino-export
├── src/
│   ├── lmpj.rs                  # 更新：支持新格式保存
│   └── project/                 # 新增
│       ├── save.rs              # 工程保存逻辑
│       └── load.rs              # 工程加载逻辑

lumino-ui
├── src/
│   └── editor/
│       └── editor_state/data.rs  # EditorData 可关联到 LuminoProject
```

---

## 10. 迁移策略

1. **第一阶段（兼容期）**：保留旧版 `encode_lmpj`/`decode_lmpj`，新增 `load_project`/`save_project` API
2. **第二阶段（过渡期）**：打开旧版文件时自动提示转换为新格式
3. **第三阶段（新格式为主）**：默认保存为新格式，继续支持读取旧格式
