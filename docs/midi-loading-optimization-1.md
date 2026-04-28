# Lumino MIDI 加载架构改造代码审查报告

## 审查范围

提交：`84698fc refactor: 按照Kimi建议进行加载内存优化`
涉及文件：9 个，+598 行 / -53 行

---

## 🔴 严重问题（会导致编译失败或运行时崩溃）

### 1. `Drop` 实现了两次 — 编译错误

**位置**：`crates/cache/src/lib.rs` 第 133-139 行 和 第 306-312 行

```rust
// 第一个 Drop（原来的）
impl Drop for MidiCache {
    fn drop(&mut self) {
        if let Some(ref path) = self._tmp_chunk_path {
            let _ = std::fs::remove_file(path);
        }
    }
}

// ... 中间插入了 from_notes_file、get_track_notes 等方法 ...

// 第二个 Drop（新增的，和第一个完全一样）
impl Drop for MidiCache {
    fn drop(&mut self) {
        if let Some(ref path) = self._tmp_chunk_path {
            let _ = std::fs::remove_file(path);
        }
    }
}
```

**后果**：Rust 不允许为同一类型实现两次相同的 trait。编译器会报错：
```
error[E0119]: conflicting implementations of trait `Drop` for type `MidiCache`
```

**修复**：删除第二个 `impl Drop`（第 306-312 行），保留第一个。

---

### 2. `build_chunks_from_notes` 的内存峰值严重低估 — OOM 风险

**位置**：`crates/cache/src/chunk.rs` 第 562-567 行（文档注释）

文档声称：
> 峰值内存：~1.5GB（1GB 文件 + 500MB 音符）

实际峰值（1GB 文件，1 亿音符）：

| 数据 | 大小 | 说明 |
|------|------|------|
| `file_bytes: Vec<u8>` | ~1 GB | `fs::read` 全量读入 |
| `notes: Vec<PackedNote>` | ~1.2 GB | 1 亿 × 12 字节 |
| `events: Vec<(u32, CompactEvent)>` | **~3.2 GB** | 2 亿 × 16 字节（NoteOn + NoteOff） |
| `chunk_map: BTreeMap` | ~0.5 GB | 节点开销 + Vec 分配 |
| **同时存在的峰值** | **~5.9 GB** | file_bytes + notes + events |

**分析**：

`from_notes_file` 的执行时序：
```
1. file_bytes = fs::read(path)        // 1GB
2. notes = extract_notes_from_bytes(&file_bytes)  // +1.2GB = 2.2GB
3. drop(file_bytes)                    // -1GB = 1.2GB  ← drop 太早了吗？不，是对的
4. events = Vec::with_capacity(notes.len() * 2)    // 分配 2亿 × 16B = 3.2GB
5. 填充 events（从 notes 迭代）         // 同时持有 notes + events = 4.4GB
6. events.sort_unstable()              // sort 可能额外分配 ~2GB（tim sort 的临时 Vec）
   → 峰值可能达到 **6-7GB**
```

相比之下，原来的 `MidiCache::load()`（桶排序方案）：
```
1. mmap 文件（虚拟内存映射，不占用 RSS）
2. phase1_bucketize：只保留当前 event + 桶文件（64 个桶）
   → 峰值：桶文件总大小（~1GB）+ 少量缓冲
3. drop(file_data)                     // 释放 mmap
4. phase2_assemble：逐桶读取，构建 chunk
   → 峰值：一个桶的数据（~16MB）+ 当前 chunk
5. 最终峰值 **~1.5GB**
```

**结论**：`from_notes_file` 的内存峰值（6-7GB）比原来的桶排序方案（~1.5GB）**差了 4 倍以上**。
原来的桶排序虽然是两遍解析，但用外部排序做到了流式处理，内存反而更低。

**修复建议**：不要一次性构建 `events` Vec。改为流式处理：
```rust
// 方案 A：从 PackedNote 直接流式写入 chunk（不需要 events Vec）
// 1. 按 start_tick 排序 notes（PackedNote 可排序）
// 2. 流式遍历，每个 tick 范围的 notes 直接转为 chunk 写入

// 方案 B：保留桶排序，但用 extract_notes_from_bytes 替代 mmap+midly::parse
// 1. notes = extract_notes_from_bytes(&file_bytes)
// 2. 按 tick 分发到 64 个桶文件（不需要 events Vec）
// 3. phase2 从桶文件构建 chunk
```

---

### 3. `get_track_notes` 的性能灾难 — 编辑器卡死

**位置**：`crates/cache/src/lib.rs` 第 226-269 行

```rust
pub fn get_track_notes(&self, track_id: u16) -> Vec<(f32, u8, f32, u8)> {
    let events = self.get_track_events(track_id);  // ← 问题在这里
    // ...
}

pub fn get_track_events(&self, track_id: u16) -> Vec<CompactEvent> {
    let total = self.index.total_ticks;
    let mut events = self.cache.get_events(0, total, 0);  // ← 全量加载所有 chunk！
    events.retain(|ev| ev.track_id() == track_id);  // ← 内存中筛选
    events
}
```

**问题**：
1. `get_events(0, total_ticks, 0)` 会遍历**所有 chunk**，从后端读取全部事件
2. 对于 100 个音轨，`import_midi_to_editor` 会调用 100 次 `get_track_notes`
3. 每次都要重新从磁盘读取全部 chunk → **100 次全量 I/O**

**后果**：加载一个 1GB 的 MIDI 到编辑器，I/O 量 = 100 × 1GB = **100GB 磁盘读取**。编辑器会卡死数十秒。

**修复建议**：
```rust
// 方案 A：利用 chunk 的 track_mask 跳过不含该音轨的 chunk
pub fn get_track_events(&self, track_id: u16) -> Vec<CompactEvent> {
    let mut events = Vec::new();
    let track_bit = 1u64 << (track_id as u64 % 64);
    let track_mask_idx = if track_id < 64 { 0 } else { 1 };

    for entry in self.index.entries.iter() {
        // 只读取包含该音轨的 chunk
        if entry.track_mask[track_mask_idx] & track_bit != 0 {
            let chunk_events = self.cache.get_events(
                entry.start_tick,
                entry.start_tick + CHUNK_TICK_SPAN,
                0,
            );
            events.extend(chunk_events.into_iter().filter(|ev| ev.track_id() == track_id));
        }
    }
    events
}

// 方案 B：新增音轨级别的索引（在构建时预计算每个音轨的 chunk 列表）
```

---

### 4. `get_tempo_changes` 同样全量加载

**位置**：`crates/cache/src/lib.rs` 第 271-297 行

```rust
pub fn get_tempo_changes(&self) -> Vec<(u32, f32)> {
    let total = self.index.total_ticks;
    let events = self.cache.get_events(0, total, 0);  // ← 又是全量加载！
    // ...
}
```

Tempo 事件通常很少（几十个到几百个），但每次都要读取全部 chunk。

**修复建议**：在 `MidiCache` 构建时预提取 tempo_changes 并存入单独的字段，不需要从 cache 读取。

---

## 🟡 架构问题（设计层面的缺陷）

### 5. 改造只走了一半 — 标准模式仍然双重解析

**位置**：`crates/core/src/midi/loader/parsed_midi.rs` 第 122-217 行

`skip_memory_manager = false`（标准模式）的加载流程**完全没有改动**：

```rust
// 标准模式：第 122-217 行，原封不动
let manager = MidiMemoryManager::load(&path_clone, &cache_dir, ...)?;  // ← 第一重全量解析
// ...
let cache = match lumino_cache::MidiCache::load(&path_for_cache, None) {  // ← 第二重全量解析
```

这意味着：
- 用户在菜单中点击"加载 MIDI"（标准模式）→ **仍然 5-8GB 峰值内存**
- 只有点击"加载 MIDI（内存优化）" → 才会走新的 `from_notes_file` 路径
- **但 `from_notes_file` 的实际峰值是 6-7GB（见问题 2），比标准模式更差！**

**修复建议**：统一两条路径都用 `from_notes_file`（但要先修复问题 2 的内存问题）。或者反过来：让标准模式也用新路径，然后删除 `MidiMemoryManager`。

---

### 6. 洋葱皮在 cache-only 模式仍然失效

**位置**：`src/runner/midi_handler.rs` 第 67-74 行

```rust
// 只有 memory_manager 路径才预加载洋葱皮
for (track_idx, _, note_count) in &track_infos {
    if *note_count > 0 {
        self.preload_track_for_onion_skin(&mut memory_manager, *track_idx, ui);  // ← cache 路径没有这个调用！
    }
}
```

cache-only 路径（第 87-136 行）没有对应的 `preload_track_for_onion_skin` 调用。

**修复建议**：在 cache 路径中也添加洋葱皮预加载（但要先用 track_mask 优化 `get_track_notes`，否则性能太差）。

---

### 7. 音轨名称丢失

**位置**：`src/runner/midi_handler.rs` 第 100 行

```rust
track_infos.push((track_idx, None, note_count));  // ← 音轨名永远是 None
```

原来 memory_manager 路径会读取 `TrackName` 元事件：
```rust
let track_name = events.iter().find_map(|e| {
    if let MidiEvent::TrackName { name, .. } = e { Some(name.clone()) } else { None }
});
```

但 `build_chunks_from_notes` 没有将 TrackName 元事件存入 chunk，`get_track_events` 也无法读取到。

**修复建议**：在 `extract_notes_from_bytes` 之外，额外扫描一次 TrackName 事件。或者在 `MidiCache` 构建时预提取并存储音轨名列表。

---

### 8. 没有使用 StreamingNoteLoader — 播放路径未改造

这是之前建议中最核心的部分之一，但本次提交**完全没有涉及**。

当前播放仍然使用 `MidiEventStream`（ouroboros 自引用结构），这需要在内存中保留完整的 `Smf` 解析结果。对于 1GB+ 文件，这会占用数 GB 内存。

`StreamingNoteLoader` 的优势：
- mmap 文件，零拷贝
- `prepare_frame(tick, window)` 只返回可见音符
- 有界内存（可配置 `max_finished_notes`）

---

### 9. Tempo 事件与其他事件混合存储

**位置**：`crates/cache/src/chunk.rs` 第 601-618 行

```rust
// Tempo 事件被放在 track 0
CompactEvent::new(tick, 0, EventKind::Tempo, 0, ...)
```

Tempo 是全局事件，但被硬编码为 track 0。这意味着：
- `get_track_notes(0)` 返回的事件中会混入 Tempo 事件（虽然配对逻辑会跳过，但增加了不必要的遍历）
- 如果真实音轨 0 的事件和 Tempo 事件在同一个 chunk，会一起被加载

---

## 🟢 轻微问题

### 10. `events.sort_unstable` 对 2 亿个元素排序

**位置**：`crates/cache/src/chunk.rs` 第 620 行

```rust
events.sort_unstable_by_key(|(tick, _)| *tick);
```

2 亿个 16 字节元素的排序：
- 时间复杂度 O(N log N) ≈ 2亿 × 28 ≈ **56 亿次比较**
- 内存：排序可能额外分配 50% 的缓冲区（tim sort）
- 耗时：在普通 SSD 上可能需要 **30-60 秒**

原来的桶排序方案没有排序（桶编号即 tick 范围，天然有序）。

**修复建议**：用计数排序/基数排序替代（tick 是 u32，可以用 2048 个 bucket 做基数排序）。或者直接分发到桶文件（回到原来的 phase1 思路）。

---

### 11. `BTreeMap` 可以用 `HashMap` 替代

**位置**：`crates/cache/src/chunk.rs` 第 623 行

```rust
let mut chunk_map: std::collections::BTreeMap<u32, Vec<CompactEvent>> = ...;
```

`chunk_idx` 是整数，不需要有序。`HashMap` 的 `entry().or_default()` 更快。

不过这里最终要按 chunk_idx 排序写入文件，所以如果用 HashMap，最后需要 `into_iter().collect::<Vec<_>>().sort_by_key()`。差别不大，但 BTreeMap 的每次插入有 O(log N) 的树旋转开销。

---

### 12. `track_count` 计算可能不准确

**位置**：`crates/cache/src/chunk.rs` 第 589 行

```rust
track_count = track_count.max(track.saturating_add(1));
```

如果 MIDI 文件的音轨编号不连续（例如 track 0, 2, 5），`track_count` 会是 6 而不是 3。

不过实际中音轨编号通常是连续的，这个问题很少触发。

---

### 13. `memmap.rs` 的 `_temp_path` 类型不一致

**位置**：`crates/cache/src/backend/memmap.rs`

```rust
// 原来
_temp_path: Option<std::path::PathBuf>,   // 字段声明
Some(temp_path)                           // create_temp_file 返回

// 改造后
_temp_path: Some(temp_path),              // 直接存 PathBuf
// 但 create_temp_file 返回的是 PathBuf（不是 Option<PathBuf>）
```

看起来类型对不上。需要确认 `MemmapBackend` 结构体定义是否也改了。

---

## 📊 问题总结

| 严重程度 | 数量 | 问题 |
|---------|------|------|
| 🔴 严重 | 4 | 编译错误(Drop×2)、内存峰值低估(6-7GB vs 声称1.5GB)、get_track_notes 100次全量I/O、get_tempo_changes全量加载 |
| 🟡 架构 | 5 | 标准模式未改造、洋葱皮仍失效、音轨名丢失、未用StreamingNoteLoader、Tempo混在track0 |
| 🟢 轻微 | 4 | 2亿元素排序慢、BTreeMap可换HashMap、track_count计算、_temp_path类型 |

**最关键的决策**：

本次改造试图用 `extract_notes_from_bytes` + `build_chunks_from_notes` 替代桶排序，但：
1. 新方案的内存峰值（6-7GB）**比原桶排序方案（~1.5GB）更差**
2. 新方案的编辑器性能（100次全量 I/O）**不可接受**
3. 只改了"内存优化"模式，标准模式**完全没动**

**建议回退 `build_chunks_from_notes`，改用以下方案**：

```
// 最优方案：保留桶排序的流式架构，但用 extract_notes_from_bytes 替代 mmap+midly::parse
from_notes_file(path):
  1. file_bytes = fs::read(path)           // 1GB
  2. notes = extract_notes_from_bytes(&file_bytes)  // +1.2GB
  3. drop(file_bytes)                      // -1GB，剩余 1.2GB
  4. 流式遍历 notes，按 tick 分发到 64 个桶文件（不创建 events Vec）
     → 峰值维持 1.2GB + 桶缓冲（~16MB）
  5. phase2_assemble 从桶文件构建 chunk（复用原代码）
     → 峰值 ~1.3GB
```

这样改造后：
- **只解析一次**（用 midly-fork 的 extract_notes）
- **峰值内存 ~1.3GB**（比原来标准模式的 5-8GB 大幅降低）
- **保留流式桶排序**（不构建巨大的 events Vec）
- **统一标准模式和优化模式**（都用 from_notes_file）
- **删除 MidiMemoryManager**（后续步骤）
