# lumino-cache — MIDI 数据分层缓存系统

专为黑乐谱（black MIDI）超大文件设计的缓存系统。支持 1GB+ 的 .mid 文件，内存可控在 1GB 以内。

## 架构

```
┌─────────────────────────────────────────┐
│            TrackDataRef                  │  ← 音轨视图（不持有数据）
├─────────────────────────────────────────┤
│         LayeredCache (L1+L2+L3)         │  ← 三层缓存
│  ┌───────┐ ┌────────┐ ┌───────────┐    │
│  │ L1    │ │ L2     │ │ L3        │    │
│  │ Hot   │ │ Chunk  │ │ Page      │    │
│  │ Cache │ │ Cache  │ │ Backend   │    │
│  └───────┘ └────────┘ └───────────┘    │
├─────────────────────────────────────────┤
│  ChunkIndex (常驻内存) + Prefetch Thread │
├─────────────────────────────────────────┤
│        PageBackend (跨平台抽象)          │
│  Windows: VirtualAlloc | Linux/macOS: mmap │
└─────────────────────────────────────────┘
```

## 数据流

1. `chunk_midi_data()` 解析 .mid → `EventChunk[]` + `ChunkIndex`
2. `ChunkIndex` 常驻内存（~160KB 每 5000 块）
3. EventChunk 序列化后存储在 PageBackend (L3)
4. L2 按 LRU 缓存反序列化的 EventChunk
5. L1 缓存播放窗口附近 ±2 秒的 CompactEvent
6. 预取线程异步加载当前块前方 4 块到 L2

## 内存占用

| 层级 | 内容 | 典型大小 |
|------|------|----------|
| ChunkIndex | 块索引条目（32 字节/条） | 5000 块 ≈ 160 KB |
| L1 HotCache | CompactEvent（12 字节/事件） | 100K 事件 ≈ 1.2 MB |
| L2 ChunkCache | EventChunk（反序列化） | 128 块 ≈ 256 MB - 1 GB |
| L3 PageBackend | 原始序列化字节 | Windows 上 LRU 可控 |

## 调优参数

所有可调参数在 `params` 模块中，详见 `tuning.md`。

## 快速开始

```rust
use lumino_cache::MidiCache;

// 加载 MIDI 文件
let cache = MidiCache::load("path/to/file.mid", Some(&|p| {
    println!("Loading: {:.1}%", p * 100.0);
}))?;

// 获取 tick 范围的事件
let events = cache.cache.get_events(0, 65536);

// 管理音轨视图
cache.tracks.set_visibility(5, TrackVisibility::Muted);

// 获取性能指标
println!("{}", cache.metrics.report());
```

## 依赖关系

- `lumino-midi` — CompactEvent, EventKind 定义
- `midly` — MIDI 文件解析
- `bincode` — EventChunk 序列化

## 平台支持

- **Windows** — 使用 `VirtualAlloc` 分页缓存，64KB/页，LRU 淘汰，显式 shrink
- **Linux/macOS** — 使用 `memmap2` 全量 mmap，由操作系统管理换页
