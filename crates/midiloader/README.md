# Lumino MIDI Loader

高性能 MIDI 文件解析库，支持标准 MIDI 文件格式（SMF）。

## 特性

- **内存映射**: 使用 `memmap2` 实现高效文件读取
- **零拷贝模式**: 支持直接引用内存映射区域，避免不必要的数据复制
- **全格式支持**: 支持 MIDI 格式 0、1、2
- **进度报告**: 支持加载进度回调
- **并行加载**: 适合多线程环境下批量处理

## 快速开始

### 基本用法（内存加载）

```rust
use lumino_midiloader::load;

let midi = load("song.mid")?;
println!("轨道数: {}", midi.track_count());
println!("事件数: {}", midi.total_events());
```

### 零拷贝模式（推荐用于大文件）

```rust
use lumino_midiloader::{MmapReader, MmapMidiLoader};

let reader = MmapReader::open("song.mid")?;
let loader = MmapMidiLoader::new();
let midi = loader.load(&reader)?;

// 遍历事件
for track in &midi.tracks {
    for event in track.iter_mmap_events() {
        println!("{:?}", event.kind);
    }
}
```

### 带进度报告

```rust
use lumino_midiloader::{LoadOptions, MidiLoader};

let (options, handle, reporter) = LoadOptions::new().with_progress();
let loader = MidiLoader::with_options_and_reporter(options, reporter);

// 在另一个线程接收进度
std::thread::spawn(move || {
    while let Ok(event) = handle.recv() {
        println!("进度: {:?}", event);
    }
});

let midi = loader.load("song.mid")?;
```

### 快速分析（仅统计）

```rust
use lumino_midiloader::{MmapReader, MmapMidiLoader};

let reader = MmapReader::open("song.mid")?;
let loader = MmapMidiLoader::new();

let mut note_count = 0;
let mut bpm = 120.0;

loader.analyze_streaming(&reader, |track, _index| {
    use lumino_midiloader::mmap_model::FastEventKind;
    
    for event in track.iter_fast_events() {
        match event.kind {
            FastEventKind::NoteOn { .. } => note_count += 1,
            FastEventKind::Tempo { tempo } => {
                bpm = 60_000_000.0 / tempo as f64;
            }
            _ => {}
        }
    }
    Ok(())
})?;

println!("音符数: {}, BPM: {}", note_count, bpm);
```

## 核心类型

### 数据模型

- `MidiFile`: MIDI 文件结构
- `Track`: MIDI 轨道
- `Event`: MIDI 事件
- `Note`: 音符信息（音符号 + 速度）
- `MetaEvent`: 元事件（速度、拍号、调号等）

### 加载器

- `MidiLoader`: 标准加载器（加载到内存）
- `MmapMidiLoader`: 零拷贝加载器（基于内存映射）

### 读取器

- `MmapReader`: 内存映射文件读取器
- `ByteBuffer`: 字节缓冲区读取器

## 运行示例

```bash
cargo run --example midi_test -- file1.mid file2.mid
```

## 许可证

MIT
