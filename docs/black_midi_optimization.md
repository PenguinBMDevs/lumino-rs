# 0.6B音符 × 千音轨 黑乐谱性能诊断与高ROI优化

> 针对 600,000,000 音符 / 1000+ 音轨 / 无空白音轨的极端负载场景

---

## 1. 数据画像：你在处理什么

| 指标 | 数值 | 含义 |
|------|------|------|
| 音符总数 | **600,000,000** | 7.2 GB 的 `PackedNote` Vec |
| 音轨数 | **1,000+** | 每轨平均 60 万音符 |
| `PackedNote` Vec | **7.2 GB** | 超出 L3 缓存 (64MB) **115 倍**，随机访问 = 100% cache miss |
| `track_note_indices` | **4.8 GB** | 纯临时索引，用完即丢 |
| `CompactEvent` (events) | **14.4 GB** | NoteOn + NoteOff 各 6 亿 |
| `NoteInfo` (cache) | **9.6 GB** | 预解析音符缓存 |
| **内存峰值** | **~36 GB** | 四段大 Vec 同时存在 |
| **总内存流量** | **~67 GB** | 5 次全量遍历 |

### 1.1 内存带宽瓶颈的硬物理极限

```
DDR5-5600 理论带宽: ~90 GB/s
DDR5-5600 实测顺序带宽: ~70-80 GB/s  
DDR5-5600 实测随机带宽: ~2-5 GB/s（cache miss 到主存 ~100ns/次）
```

67 GB 流量 @ 80 GB/s 顺序 = **0.8 秒**（理论下限）

67 GB 流量 @ 3 GB/s 随机 = **22 秒**（随机访问的实际时间）

**结论：当前架构不是受限于"计算"，而是受限于"随机访问导致的内存带宽坍缩"。**

---

## 2. 热点精确分解：每一秒花在哪

### 2.1 第1热点：`convert_notes_parallel` — 随机访问地狱（~60秒）

**位置：** `document.rs:209-248`

```rust
// ① 构建索引：6亿次 push，1000个Vec各自独立扩容
for (idx, note) in notes.iter().enumerate() {
    track_note_indices[note.track as usize].push(idx);
}

// ② par_iter中：随机访问 7.2GB 的 notes[idx]
track_note_indices.par_iter().map(|indices| {
    for &idx in indices {
        let note = &notes[idx];  // ← 6亿次cache miss到主存
        // ...
    }
}).collect()
```

**为什么这是地狱：**

- `notes` 是 7.2 GB 的 Vec，跨度超过 L3 缓存 115 倍
- `idx` 不是连续的（音符按 `par_iter` 调度顺序写入，跨轨交错）
- 即使同轨的 `idx` 连续，7.2 GB 的跨度意味着每次访问都大概率 cache miss
- 6 亿次 cache miss × 100 ns = **60 秒**

**对比：** 同样的数据量，顺序扫描仅需 **12 秒**（6 亿 / 5 notes-per-cacheline × 100 ns）

**加速比：顺序 vs 随机 = 5x**

### 2.2 第2热点：1000次排序（~23秒）

**位置：** `document.rs:244`

```rust
events.sort_by_key(|e| e.delta_tick());  // ← 1000次，每轨60万音符
```

MIDI 文件的音符**本来就是按 tick 有序的**（绝大多数情况下）。`sort_by_key` 在这里是对已有序数据的浪费。

1000 × O(600K log 600K) ≈ **23 秒纯排序时间**

### 2.3 第3热点：3次全量扫描（~10秒 + cache压力）

| 函数 | 操作 | 流量 |
|------|------|------|
| `build_track_statistics` | 顺序读 7.2 GB | 7.2 GB |
| `convert_notes_parallel` | 随机读 7.2 GB + 写 14.4 GB | 21.6 GB |
| `build_note_cache` | 顺序读 7.2 GB + 写 9.6 GB | 16.8 GB |
| `merge_events_with_tempos`| 拷贝 14.4 GB | 14.4 GB |

三次 PackedNote 扫描 = 3 × 7.2 GB = **21.6 GB 读取流量**

### 2.4 第4热点：2000个独立 Vec 分配

1000 轨 × (`track_events` + `track_notes_cache`) = **2000 个独立 Vec**

每次 `Vec::with_capacity` → `malloc` → 后续可能的 `realloc`。在高频分配场景下，`malloc` 的内部锁竞争和内存碎片不可忽视。

---

## 3. 优化方案：只针对 40%+ ROI 的改动

### 方案 A：Arena + 顺序双遍扫描 — 消除随机访问（2-5x 加速）★★★

**目标：** 用两次顺序扫描 + 一个预分配 Arena 替代 `track_note_indices` 的随机访问模式。

**核心思路：**

```
当前：顺序扫描 → 索引 → 随机访问 → CompactEvent
优化：顺序扫描 → 计数 → 顺序扫描 → Arena写入 → CompactEvent
         ↑_________________________↑
              两次顺序扫描（cache友好）
```

**实现：**

```rust
fn convert_notes_parallel_arena(
    notes: &[PackedNote],
    track_note_counts: &[u64],
) -> Vec<Vec<CompactEvent>> {
    use rayon::prelude::*;
    let track_count = track_note_counts.len();

    // 第一遍：精确统计每轨音符数（顺序扫描，cache友好）
    // 注意：复用 build_track_statistics 的结果，不再重复统计
    // 假设 track_note_counts 已经已知

    // 方案 A1：用一个大 Arena + 每轨 slice 替代 1000 个独立 Vec
    let total_events: usize = track_note_counts.iter()
        .map(|&c| c as usize * 2)
        .sum();
    
    // 计算每轨在 arena 中的起始偏移
    let mut arena_offsets = Vec::with_capacity(track_count);
    let mut cumsum = 0usize;
    for &count in track_note_counts {
        arena_offsets.push(cumsum);
        cumsum += count as usize * 2;
    }

    // Arena：一次大分配，零碎片
    let mut arena: Vec<CompactEvent> = Vec::with_capacity(total_events);
    // SAFETY: 后续会精确填充到 total_events，不会越界
    unsafe { arena.set_len(total_events); }

    // 每轨的写入头（原子操作，用于并行写入）
    let mut write_heads = arena_offsets.clone();

    // 第二遍：顺序扫描 PackedNote，直接写入 Arena 的对应位置
    // 这一步是顺序读取 notes（cache友好）+ 随机写入 arena（无竞争）
    for note in notes {
        let tid = note.track as usize;
        let head = write_heads[tid];
        // SAFETY: head 在 arena_offsets[tid]..arena_offsets[tid+1] 范围内
        unsafe {
            let ptr = arena.as_mut_ptr().add(head);
            ptr.write(CompactEvent::new(
                note.start_tick, note.track, EventKind::NoteOn, 0,
                note.key as u16, note.velocity as u16,
            ));
            ptr.add(1).write(CompactEvent::new(
                note.end_tick, note.track, EventKind::NoteOff, 0,
                note.key as u16, note.velocity as u16,
            ));
        }
        write_heads[tid] = head + 2;
    }

    // 将 arena 切分为每轨的 Vec
    let mut track_events: Vec<Vec<CompactEvent>> = Vec::with_capacity(track_count);
    for i in 0..track_count {
        let start = arena_offsets[i];
        let end = if i + 1 < track_count { arena_offsets[i + 1] } else { total_events };
        // 从 arena 中提取子切片作为 Vec
        track_events.push(arena[start..end].to_vec());
    }

    track_events
}
```

**等等——这还不够好。** 上面的代码在最后一步 `arena[start..end].to_vec()` 做了 1000 次拷贝。更干净的方案是**让 MidiDocument 直接持有 Arena**，每轨用 `(start, end)` 索引：

```rust
// === MidiDocument 重构 ===
pub struct MidiDocument {
    // 所有事件存储在一个 Arena 中
    events_arena: Vec<CompactEvent>,
    // 每轨的起始/结束偏移（无需独立 Vec）
    track_event_ranges: Vec<(usize, usize)>,
    
    // NoteInfo 同理
    note_cache_arena: Vec<NoteInfo>,
    note_cache_ranges: Vec<(usize, usize)>,
    
    tempo_changes: Vec<(u32, f32)>,
    control_events: Vec<PackedControlEvent>,
    // ...
}
```

这样完全消除了 1000 次 Vec 分配和最后的拷贝。

**预期收益：**

| 指标 | 当前 | 优化后 | 加速 |
|------|------|--------|------|
| notes 访问模式 | 随机 (6亿 cache miss) | 顺序 (1.2亿 cache miss) | **5x** |
| Vec 分配次数 | 1000+ 次 | 1 次 Arena | **1000x 减少分配** |
| 最后拷贝 | 1000 次 `to_vec` | 0 次（直接 slice） | **消除** |
| 碎片 | 2000 个独立堆块 | 2 个连续大块 | **零碎片** |

**综合：convert_notes_parallel 阶段从 ~60+秒 降至 ~10-15 秒，加速 4-6x。**

### 方案 B：有序性检测 — 跳过 1000 次无用排序（10-20x 排序阶段）★★★

**位置：** `document.rs:244`

**核心洞察：** 绝大多数 MIDI 文件的音符本来就是按 `delta_tick` 有序的。`sort_by_key` 是对已有序数据的浪费。

**实现：**

```rust
// 用 O(N) 的有序性检测替代盲目的 O(N log N) 排序
track_events
    .par_iter_mut()
    .for_each(|events| {
        if !is_sorted_by_key(events, |e| e.delta_tick()) {
            events.sort_unstable_by_key(|e| e.delta_tick());
        }
    });

#[inline(always)]
fn is_sorted_by_key<T, K: PartialOrd>(slice: &[T], mut key: impl FnMut(&T) -> K) -> bool {
    if slice.len() < 2 {
        return true;
    }
    let mut prev = key(&slice[0]);
    for i in 1..slice.len() {
        let curr = key(&slice[i]);
        if curr < prev {
            return false;
        }
        prev = curr;
    }
    true
}
```

**预期收益：**

- 99%+ 的 MIDI 文件：有序性检测 **O(N)** 替代排序 **O(N log N)**
- 千轨总排序时间：23 秒 → **~1 秒**（检测时间）
- **排序阶段加速：10-20x**

**风险：** 极低。检测通过则无事发生，检测失败则正常排序。

### 方案 C：三合一扫描 — 统计+转换+缓存一次完成（30-50% 加速）★★

**位置：** `document.rs:115-176`

**当前：** 3 次独立扫描 PackedNote

```rust
let (total_ticks, track_note_counts, total_note_count) = Self::build_track_statistics(&notes);  // 第1遍
let track_events = Self::convert_notes_parallel(&notes, &track_note_counts);                      // 第2遍（随机访问）
let track_notes_cache = Self::build_note_cache(&notes, &track_note_counts);                        // 第3遍
drop(notes);  // ← 终于可以释放 7.2GB
```

**优化：** 将三个函数合并为一次遍历。但这需要重构数据流——`convert_notes` 需要先知道 `track_note_counts` 才能预分配 Arena。

**实现（依赖方案 A 的 Arena）：**

```rust
fn build_all_in_one(notes: &[PackedNote], track_count: usize) -> AllInOneResult {
    // 阶段 1：统计（必须先知道数量才能预分配 Arena）
    let mut track_counts = vec![0u64; track_count];
    let mut total_ticks: u32 = 0;
    for note in notes {
        track_counts[note.track as usize] += 1;
        total_ticks = total_ticks.max(note.end_tick);
    }

    // 阶段 2：预分配 Arena
    let total_events: usize = track_counts.iter().map(|&c| c as usize * 2).sum();
    let total_noteinfos: usize = track_counts.iter().map(|&c| c as usize).sum();
    
    let mut events_arena = vec![CompactEvent::default(); total_events];
    let mut cache_arena = vec![NoteInfo::default(); total_noteinfos];
    
    // 计算偏移
    let mut event_offsets = vec![0usize; track_count];
    let mut cache_offsets = vec![0usize; track_count];
    let mut cum_e = 0usize;
    let mut cum_c = 0usize;
    for i in 0..track_count {
        event_offsets[i] = cum_e;
        cache_offsets[i] = cum_c;
        cum_e += track_counts[i] as usize * 2;
        cum_c += track_counts[i] as usize;
    }

    let mut event_heads = event_offsets.clone();
    let mut cache_heads = cache_offsets.clone();

    // 阶段 3：一次遍历，同时写入 events_arena + cache_arena
    for note in notes {
        let tid = note.track as usize;
        let eh = event_heads[tid];
        let ch = cache_heads[tid];

        events_arena[eh] = CompactEvent::new(note.start_tick, note.track, EventKind::NoteOn, 0, note.key as u16, note.velocity as u16);
        events_arena[eh + 1] = CompactEvent::new(note.end_tick, note.track, EventKind::NoteOff, 0, note.key as u16, note.velocity as u16);
        cache_arena[ch] = NoteInfo::new(note.start_tick, note.end_tick.saturating_sub(note.start_tick), note.key, note.velocity, 0);

        event_heads[tid] = eh + 2;
        cache_heads[tid] = ch + 1;
    }

    AllInOneResult {
        events_arena,
        cache_arena,
        event_offsets,
        cache_offsets,
        track_counts,
        total_ticks,
    }
}
```

**预期收益：**

- PackedNote 扫描次数：3 次 → 2 次（统计 + 写入）
- 7.2 GB 读取流量减少 33%
- `notes` 可提前 `drop`，7.2 GB 更早释放
- **总体加速：30-50%**（主要来自于减少一次全量扫描和提前释放内存）

### 方案 D：直接从 `FastTrackIter` 构建 — 终极优化（50-70% 加速）★★

**目标：** 完全跳过 `PackedNote` 中间层。每个 `FastTrackIter` 直接输出到预分配的 Arena slot。

**为什么这是终极方案：**

当前数据流：
```
FastTrackIter → PackedNote (per track) → 全局Vec (7.2GB) → Arena → CompactEvent + NoteInfo
     ↑______音轨级并行_________↑    ↑_单线程合并_↑   ↑______再次并行______↑
```

优化后数据流：
```
FastTrackIter → 直接写入 per-track Arena slot → CompactEvent + NoteInfo
     ↑___________音轨级并行，零中间层__________↑
```

**实现（需在 midly-fork 中新增）：**

```rust
// === midly-fork: src/loader.rs ===

pub struct DirectTrackOutput {
    pub events: Vec<CompactEvent>,      // 该轨的 CompactEvent（已排序）
    pub note_infos: Vec<NoteInfo>,      // 该轨的 NoteInfo（已排序）
    pub tempo_changes: Vec<(u32, f32)>,
    pub control_events: Vec<PackedControlEvent>,
}

/// 直接从 FastTrackIter 解析到最终格式，跳过 PackedNote
pub fn extract_direct(bytes: &[u8]) -> crate::Result<Vec<DirectTrackOutput>> {
    let data = crate::ump::preprocess_smf(bytes);
    let (_header, tracks_count, _division, raw) = fast_midi::parse_header(&data)?;
    let tracks = fast_midi::iter_tracks_from_data(raw, tracks_count);

    let results: Vec<DirectTrackOutput> = {
        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            tracks
                .into_par_iter()
                .enumerate()
                .map(|(track_idx, events)| parse_direct(events, track_idx as u16))
                .collect()
        }
        #[cfg(not(feature = "parallel"))]
        {
            tracks
                .into_iter()
                .enumerate()
                .map(|(track_idx, events)| parse_direct(events, track_idx as u16))
                .collect()
        }
    };

    Ok(results)
}

fn parse_direct(mut events: FastTrackIter, track_idx: u16) -> DirectTrackOutput {
    // 先用一个可增长的 Vec 收集（无法预知容量）
    // 或者用两阶段：先快速扫描估算容量，再精确填充
    let mut compact_events: Vec<CompactEvent> = Vec::with_capacity(1024);
    let mut note_infos: Vec<NoteInfo> = Vec::with_capacity(512);
    let mut tempo_changes: Vec<(u32, f32)> = Vec::new();
    let mut control_events: Vec<PackedControlEvent> = Vec::with_capacity(64);
    
    let mut active_notes: [Option<(u32, u8)>; 256] = [None; 256];
    let mut active_keys: SmallVec<[u8; 32]> = SmallVec::new();
    let mut current_tick: u32 = 0;

    while let Some((delta, event)) = events.next_event() {
        current_tick = current_tick.saturating_add(delta);
        match event {
            MidiEvent::NoteOn { key, velocity, .. } => {
                let k = key as usize;
                if k < 256 {
                    if let Some((st, vel)) = active_notes[k].take() {
                        compact_events.push(CompactEvent::new(st, track_idx, EventKind::NoteOn, 0, key as u16, vel as u16));
                        compact_events.push(CompactEvent::new(current_tick, track_idx, EventKind::NoteOff, 0, key as u16, vel as u16));
                        note_infos.push(NoteInfo::new(st, current_tick - st, key, vel, 0));
                    }
                    if velocity > 0 {
                        if active_notes[k].is_none() { active_keys.push(key); }
                        active_notes[k] = Some((current_tick, velocity));
                    }
                }
            }
            MidiEvent::NoteOff { key, .. } => {
                let k = key as usize;
                if k < 256 {
                    if let Some((st, vel)) = active_notes[k].take() {
                        compact_events.push(CompactEvent::new(st, track_idx, EventKind::NoteOn, 0, key as u16, vel as u16));
                        compact_events.push(CompactEvent::new(current_tick, track_idx, EventKind::NoteOff, 0, key as u16, vel as u16));
                        note_infos.push(NoteInfo::new(st, current_tick - st, key, vel, 0));
                    }
                }
            }
            MidiEvent::ControlChange { channel, controller, value } => {
                control_events.push(PackedControlEvent::control_change(current_tick, track_idx, channel, controller, value));
            }
            MidiEvent::ProgramChange { channel, program } => {
                control_events.push(PackedControlEvent::program_change(current_tick, track_idx, channel, program));
            }
            MidiEvent::PitchBend { channel, bend } => {
                control_events.push(PackedControlEvent::pitch_bend(current_tick, track_idx, channel, bend));
            }
            MidiEvent::Meta { event_type: 0x51, data } if data.len() == 3 => {
                let us = ((data[0] as u32) << 16) | ((data[1] as u32) << 8) | (data[2] as u32);
                if us > 0 { tempo_changes.push((current_tick, 60_000_000.0 / us as f32)); }
            }
            _ => {}
        }
    }

    // finish：稀疏扫描活跃 key
    for &key in &active_keys {
        if let Some((st, vel)) = active_notes[key as usize].take() {
            compact_events.push(CompactEvent::new(st, track_idx, EventKind::NoteOn, 0, key as u16, vel as u16));
            compact_events.push(CompactEvent::new(current_tick, track_idx, EventKind::NoteOff, 0, key as u16, vel as u16));
            note_infos.push(NoteInfo::new(st, current_tick - st, key, vel, 0));
        }
    }

    DirectTrackOutput {
        events: compact_events,
        note_infos,
        tempo_changes,
        control_events,
    }
}
```

**预期收益：**

- **完全消除 7.2 GB PackedNote Vec**（无需分配、无需遍历、无需释放）
- **完全消除 4.8 GB track_note_indices**（无需随机访问索引）
- **减少一次全量扫描**（从 3 次降至 2 次：解析 + merge）
- **总体加速：50-70%**

### 方案 E：`.lmcache` 持久化缓存 — 终极后续加载（10-50x）★★

对 0.6B 音符的场景，首次解析可能需要 **2-5 分钟**。缓存后降至 **5-15 秒**。

```rust
// 缓存格式设计要点：
// 1. 使用 mmap 直接映射，零拷贝加载
// 2. CompactEvent 和 NoteInfo 按字节平铺，可直接 &[_] 引用
// 3. Header 包含原始文件的 hash（SHA-256），用于缓存失效检测
// 4. 小端序，保证跨平台一致性

pub fn load_with_cache(path: &Path) -> MidiDocument {
    let cache_path = path.with_extension("lmcache");
    
    if let Some(doc) = MidiDocumentCache::load(&cache_path) {
        return doc;  // 10-50x 加速路径
    }
    
    // 冷路径：全量解析
    let doc = load_from_scratch(path);
    MidiDocumentCache::write(&doc, &cache_path);
    doc
}
```

---

## 4. 组合预期：首次加载与后续加载

### 首次加载（冷缓存）

| 优化组合 | `convert`阶段 | `sort`阶段 | 总扫描 | **总时间** | **加速** |
|---------|-------------|-----------|--------|-----------|---------|
| 当前（基线） | ~60s（随机访问） | ~23s（1000次排序） | 3次 × 7.2GB | **~90-120s** | 1x |
| + 方案A（Arena+顺序扫描） | ~12s（顺序访问） | ~23s | 3次 | **~45-55s** | **2.0-2.7x** |
| + 方案B（有序性检测） | ~12s | ~1s（检测） | 3次 | **~25-35s** | **3.4-4.8x** |
| + 方案C（三合一扫描） | ~12s | ~1s | 2次 | **~20-30s** | **4.0-6.0x** |
| + 方案D（直接构建） | 0s（消除PackedNote层） | ~1s | 1次解析 | **~10-20s** | **6.0-12x** |

### 后续加载（有缓存）

| 场景 | 时间 | 加速 |
|------|------|------|
| 全量解析 | 90-120s | 1x |
| `.lmcache` mmap 加载 | **5-15s** | **10-50x** |

---

## 5. 实施路线图

### Phase 1：低 hanging fruit（1-2 天，3-5x 加速）

**方案 B（有序性检测）+ 方案 A（Arena 顺序扫描）**

- 改动量：~200 行
- 风险：极低
- 收益：3-5x

```rust
// document.rs 修改：
// 1. convert_notes_parallel：用 Arena + 顺序扫描替代随机访问
// 2. sort_by_key → is_sorted_by_key 检测
```

### Phase 2：架构重构（3-5 天，6-12x 加速）

**方案 D（直接构建）+ 方案 C（三合一扫描）**

- 改动量：~500 行（midly-fork + lumino-rs）
- 风险：中（需充分测试 note_on/note_off 配对逻辑）
- 收益：6-12x

```rust
// midly-fork：新增 extract_direct() + DirectTrackOutput
// lumino-rs：MidiDocument 重构为 Arena + range 索引
```

### Phase 3：缓存层（2-3 天，10-50x 后续加载）

**方案 E（.lmcache）**

- 改动量：~300 行
- 风险：低
- 收益：10-50x 后续加载

---

## 6. 关键代码片段：可直接使用的实现

### 6.1 有序性检测（方案 B，立即可用）

```rust
// 替换 document.rs:244
// 从：
//     events.sort_by_key(|e| e.delta_tick());
// 改为：

fn ensure_sorted_by_delta_tick(events: &mut [CompactEvent]) {
    if events.len() < 2 {
        return;
    }
    // O(N) 有序性检测
    let mut sorted = true;
    let mut prev = events[0].delta_tick();
    for i in 1..events.len() {
        let curr = events[i].delta_tick();
        if curr < prev {
            sorted = false;
            break;
        }
        prev = curr;
    }
    if !sorted {
        events.sort_unstable_by_key(|e| e.delta_tick());
    }
}
```

### 6.2 Arena 顺序扫描（方案 A，核心改动）

```rust
// 替换 document.rs:209-248 的 convert_notes_parallel

fn convert_notes_arena(
    notes: &[PackedNote],
    track_note_counts: &[u64],
) -> (Vec<CompactEvent>, Vec<(usize, usize)>) {
    let track_count = track_note_counts.len();
    
    // 计算偏移
    let mut offsets = Vec::with_capacity(track_count + 1);
    let mut cumsum = 0usize;
    offsets.push(0);
    for &count in track_note_counts {
        cumsum += count as usize * 2;
        offsets.push(cumsum);
    }
    
    // 一次大分配
    let mut arena = vec![CompactEvent::default(); cumsum];
    let mut heads: Vec<usize> = offsets[..track_count].to_vec();
    
    // 顺序扫描写入（cache友好）
    for note in notes {
        let tid = note.track as usize;
        let head = heads[tid];
        arena[head] = CompactEvent::new(note.start_tick, note.track, EventKind::NoteOn, 0, note.key as u16, note.velocity as u16);
        arena[head + 1] = CompactEvent::new(note.end_tick, note.track, EventKind::NoteOff, 0, note.key as u16, note.velocity as u16);
        heads[tid] = head + 2;
    }
    
    // 构建 ranges
    let ranges: Vec<(usize, usize)> = (0..track_count)
        .map(|i| (offsets[i], offsets[i + 1]))
        .collect();
    
    (arena, ranges)
}
```

### 6.3 build_note_cache 同时改为 Arena（配合方案 C）

```rust
fn build_note_cache_arena(
    notes: &[PackedNote],
    track_note_counts: &[u64],
) -> (Vec<NoteInfo>, Vec<(usize, usize)>) {
    let track_count = track_note_counts.len();
    
    let mut offsets = Vec::with_capacity(track_count + 1);
    let mut cumsum = 0usize;
    offsets.push(0);
    for &count in track_note_counts {
        cumsum += count as usize;
        offsets.push(cumsum);
    }
    
    let mut arena = vec![NoteInfo::default(); cumsum];
    let mut heads: Vec<usize> = offsets[..track_count].to_vec();
    
    for note in notes {
        let tid = note.track as usize;
        let head = heads[tid];
        arena[head] = NoteInfo::new(
            note.start_tick,
            note.end_tick.saturating_sub(note.start_tick),
            note.key,
            note.velocity,
            0,
        );
        heads[tid] = head + 1;
    }
    
    let ranges: Vec<(usize, usize)> = (0..track_count)
        .map(|i| (offsets[i], offsets[i + 1]))
        .collect();
    
    (arena, ranges)
}
```

---

## 7. 验证建议

```bash
# 1. 创建合成测试数据（600M音符 / 1000轨）
cargo test --release --features synthetic_benchmark

# 2. 使用 perf 验证瓶颈转移
perf record -g -- target/release/bench_load_black_midi
perf report

# 3. 关键指标对比
#    - cache-misses: 基线 ~6亿 → 优化后 ~1.2亿（5x）
#    - page-faults: 基线 ~900万 → 优化后 ~200万（4.5x）  
#    - 总加载时间: 基线 ~120s → 优化后 ~15-25s（5-8x）
```
