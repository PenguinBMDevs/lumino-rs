# Lumino MIDI 加载架构问题诊断与改造方案

## 一、当前架构的核心问题

### 1.1 内存占用爆表的根因：双重全量解析

**标准模式（skip_memory_manager = false）的加载流程：**

```
load_parsed_midi()
  ├── MidiMemoryManager::load()          ← 第一重全量解析
  │     ├── std::fs::read(file) → Vec<u8>占 1GB+
  │     ├── midly::parse(&data) → 懒迭代器
  │     ├── event_iters.collect() → 解包所有音轨头
  │     └── 逐轨解析为 Vec<MidiEvent> → 每个事件 24+ 字节
  │         → 1亿事件 ≈ 2.4GB+ 内存
  │     └── zstd 压缩写入磁盘缓存
  │
  └── lumino_cache::MidiCache::load()    ← 第二重全量解析
        ├── memmap2::Mmap::map(file) → 又映射 1GB
        ├── midly::parse(file_data) → 又解析一次
        ├── phase1_bucketize() → 创建 CompactEvent → 64桶外部排序
        └── phase2_assemble() → 写 chunk 文件
```

**一个 1GB 的 MIDI 文件，加载峰值内存：**
| 阶段 | 内存占用 | 说明 |
|------|---------|------|
| file_data Vec | ~1GB | fs::read 全量读入 |
| midly 解析中间态 | ~0.5GB | TrackIter + EventIter |
| Vec&lt;MidiEvent&gt; × N轨 | ~2-4GB | 24字节/事件，同时存在内存+发送channel |
| memmap 映射 | ~1GB | MidiCache 又映射一次 |
| CompactEvent 桶数据 | ~0.5GB | phase1 桶文件 |
| **峰值总计** | **5-8GB** | 两个系统各自独立工作 |

**核心问题：两个系统（MidiMemoryManager + lumino-cache）各自独立做完整的 MIDI 解析，没有共享任何中间结果。**

### 1.2 缓存优化模式的 Bug 清单

当 `skip_memory_manager = true` 时：

| Bug | 位置 | 影响 |
|-----|------|------|
| **编辑器不加载任何音符** | `midi_handler.rs:87-108` | 只设置音轨列表，不调用 load_track_notes |
| **切换音轨不加载数据** | `menu/file.rs:109-115` | 只调用 set_current_track，不加载音符 |
| **洋葱皮完全失效** | `midi_handler.rs:69-74` | 被 if let Some(memory_manager) 跳过 |
| **Tempo 变化未加载** | `midi_handler.rs:65` | load_tempo_changes_from_memory_manager 未被调用 |
| **total_notes 始终为 0** | `parsed_midi.rs:105` | 大文件不统计，info 数据不完整 |
| **quick_scan 用 mmap** | `parsed_midi.rs:17-18` | 只读 header 却映射整个文件，且 count track_iters 不精确 |

**根本原因：所有编辑器功能都假设 `memory_manager` 存在，没有为 cache-only 模式实现等效路径。**

---

## 二、改造方案：统一为 Midly-Fork + Cache 架构

### 2.1 核心原则

1. **只解析一次**：用 midly-fork 的零分配 API 提取音符，直接输入 lumino-cache
2. **移除 MidiMemoryManager**：它的功能（按音轨访问、磁盘缓存、LRU）可以被替代
3. **编辑器从 Cache 读取**：不再维护独立的内存数据结构
4. **用 StreamingNoteLoader 做播放**：惰性、有界内存、按 tick 切片

### 2.2 改造后的架构

```
load_parsed_midi()
  ├── midly::loader::scan_midi_file()     ← 一次快速扫描
  │     → 获取: track_count, note_count, tempo_changes, max_tick, division
  │     → 有界内存，不解析事件细节
  │
  ├── midly::loader::extract_notes_from_bytes()  ← 一次提取音符
  │     → 返回: Vec<PackedNote> (12字节/音符) + tempo_changes
  │     → 零中间分配，自动并行
  │     → 直接转换为 CompactEvent 写入 lumino-cache
  │
  └── lumino_cache::MidiCache::from_notes()      ← 直接构建
        → 不再需要 phase1/phase2 的桶排序
        → PackedNote → CompactEvent 直接分块

播放时:
  └── StreamingNoteLoader::open(path)
        → mmap 文件，惰性解析
        → prepare_frame(current_tick, window) → 可见音符

编辑时:
  └── 从 MidiCache 按音轨读取 CompactEvent
        → 筛选 NoteOn/NoteOff 重建音符
        → 或直接从 PackedNote 索引加载
```

### 2.3 具体改造步骤

#### Step 1: 用 `scan_midi_file` 替代 `quick_scan_midi_header`

```rust
// 改造前：只返回 (division, track_count)，且用 mmap
fn quick_scan_midi_header(path: &Path) -> Result<(u16, u16)> {
    let data = unsafe { memmap2::Mmap::map(&file)? };  // ❌ 映射整个文件
    let (header, track_iters) = midly::parse(&data)?;
    let track_count = track_iters.count() as u16;  // ❌ 不精确
    ...
}

// 改造后：获取完整文件信息，有界内存
use midly::loader::scan_midi_file;

fn scan_midi_info(path: &Path) -> Result<MidiFileInfo> {
    let result = scan_midi_file(path)?;  // ✅ 顺序读取，峰值 < 10MB
    Ok(MidiFileInfo {
        track_count: result.track_count,
        note_count: result.note_count,
        duration_ticks: result.max_tick,
        division: result.division,
        tempo_changes: result.tempo_changes,  // ✅ 顺便获取 tempo
    })
}
```

#### Step 2: 移除 `MidiMemoryManager`，统一加载入口

```rust
pub async fn load_parsed_midi(
    path: PathBuf,
    progress: Option<&ProgressCallback>,
) -> Result<ParsedMidi> {
    // 1. 快速扫描获取文件信息
    let file_info = scan_midi_info(&path)?;

    // 2. 初始化 cache（一次性处理）
    let cache = MidiCache::load(&path, |p| progress(p))?;  // 内部用 midly-fork 提取

    // 3. 构建 ParsedMidi（不再有 memory_manager）
    Ok(ParsedMidi {
        info: MidiInfo {
            path,
            track_count: file_info.track_count,
            total_notes: file_info.note_count,
            duration_ticks: file_info.duration_ticks,
            division: file_info.division,
            ...
        },
        cache: Some(Arc::new(cache)),
        // memory_manager: REMOVED
    })
}
```

#### Step 3: 改造 `MidiCache::load` 使用 `extract_notes_from_bytes`

```rust
// crates/cache/src/lib.rs
impl MidiCache {
    pub fn load<P: AsRef<Path>>(path: P, progress: ...) -> Result<Self> {
        let bytes = std::fs::read(path)?;

        // ✅ 用 midly-fork 一次性提取所有 PackedNote
        let (notes, tempo_changes) = midly::loader::extract_notes_from_bytes(&bytes)?;

        // 直接分块，不需要 phase1/phase2 桶排序
        let chunks = build_chunks_from_notes(&notes)?;
        let index = ChunkIndex::from_chunks(&chunks);

        // ...
    }
}
```

**为什么可以移除桶排序？**
- 原来桶排序是为了避免 `phase1_bucketize` 中 CompactEvent 全量留内存
- `extract_notes_from_bytes` 已经做了零分配提取，返回的 `PackedNote` 就是紧凑格式
- 可以直接遍历 PackedNote，按 tick 分块，不需要中间桶文件

#### Step 4: 编辑器从 Cache 加载音轨

```rust
// src/runner/midi_handler.rs
pub fn load_track_to_editor(
    &self,
    cache: &lumino_cache::MidiCache,  // ✅ 接受 cache 而不是 memory_manager
    track_idx: usize,
    ui: &mut Host,
) {
    // 从 cache 读取该音轨的所有事件
    let events = cache.get_track_events(track_idx);
    let notes = events_to_notes(&events);
    ui.load_track_notes(track_idx, &notes);
}
```

#### Step 5: 播放用 `StreamingNoteLoader`

```rust
// 播放器初始化
let mut loader = StreamingNoteLoader::open(&midi_path)?;
loader.set_max_finished_notes(100_000);  // 配置内存上限

// 每帧调用
let (notes, active_keys) = loader.prepare_frame(current_tick, ticks_per_screen);
```

### 2.4 需要保留和移除的代码

| 代码 | 操作 | 原因 |
|------|------|------|
| `MidiMemoryManager` | **移除** | 功能被 midly-fork + cache 替代 |
| `DiskTrackCache` | **移除** | cache 自带文件后端 |
| `load_midi_data()` | **移除** | 双重解析的根因 |
| `parse_track_events_from_iter()` | **移除** | midly-fork 直接提取 |
| `spawn_disk_writer()` | **移除** | 不再需要 |
| `decide_track_storage()` | **移除** | 不再做内存/磁盘分层 |
| `lumino-cache` 的 chunk 系统 | **保留** | 播放缓存仍需按 tick 分块 |
| `phase1_bucketize/phase2_assemble` | **简化** | 用 extract_notes 替代流式桶排序 |
| `CompactEvent` | **保留** | 12字节格式仍是最优缓存格式 |
| `MidiEventStream` | **移除** | 用 StreamingNoteLoader 替代 |
| `event/stream.rs` | **移除** | ouroboros 自引用不再需要 |
| `midly-fork::StreamingNoteLoader` | **新增使用** | 播放时惰性加载 |
| `midly-fork::extract_notes_from_bytes` | **新增使用** | 一次性提取音符 |
| `midly-fork::scan_midi_file` | **新增使用** | 快速文件扫描 |

### 2.5 内存对比

| 场景 | 改造前 | 改造后 |
|------|--------|--------|
| 加载 1GB MIDI | 5-8GB 峰值 | ~1.5GB 峰值 |
| 播放时常驻 | MidiMemoryManager + Cache | StreamingNoteLoader (可配置 < 500MB) |
| 编辑器加载音轨 | 从 memory_manager Vec 复制 | 从 cache 页读取 |
| 代码行数 | ~1500 行 (managed_midi + loader) | ~300 行 (使用 midly-fork API) |

---

## 三、关键 API 对照

### Midly-Fork 提供的功能 vs 当前自定义代码

| 需求 | 当前实现 | Midly-Fork 替代方案 |
|------|---------|-------------------|
| 按 tick 范围获取可见音符 | 无（全量加载） | `StreamingNoteLoader::prepare_frame()` |
| 提取所有音符 | `parse_smf_to_notes()` 手动遍历 | `extract_notes_from_bytes()` 零分配 |
| 快速文件扫描 | `quick_scan_midi_header()` mmap | `scan_midi_file()` 顺序读 < 10MB |
| 紧凑音符格式 | `CompactEvent` 12字节 | `PackedNote` 12字节（可直接转换）|
| 空间索引 | `ChunkIndex` 按 tick 分块 | `NoteIndex` 按 chunk 索引 |
| 惰性流式加载 | `MidiEventStream` (ouroboros) | `StreamingNoteLoader` (mmap) |
| 并行解析 | 无 | `extract_notes()` 自动 rayon 并行 |
