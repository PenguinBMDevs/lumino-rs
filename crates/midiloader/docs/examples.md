# 示例代码

## 基础用法

### 加载 MIDI 文件

```rust
use lumino_midiloader::load;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let midi = load("path/to/song.mid")?;
    
    println!("格式: {}", midi.header.format);
    println!("轨道数: {}", midi.track_count());
    println!("总事件数: {}", midi.total_events());
    
    Ok(())
}
```

### 遍历轨道和事件

```rust
use lumino_midiloader::load;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let midi = load("song.mid")?;
    
    for (i, track) in midi.tracks.iter().enumerate() {
        println!("轨道 {}: {:?}", i, track.name);
        
        for event in &track.events {
            match &event.kind {
                lumino_midiloader::EventKind::NoteOn(note) => {
                    println!("  Note On: key={}, velocity={}", note.key, note.velocity);
                }
                lumino_midiloader::EventKind::NoteOff(note) => {
                    println!("  Note Off: key={}", note.key);
                }
                lumino_midiloader::EventKind::Meta(meta) => {
                    println!("  Meta: {:?}", meta.type_name());
                }
                _ => {}
            }
        }
    }
    
    Ok(())
}
```

## 进度报告

### 显示加载进度

```rust
use lumino_midiloader::{MidiLoader, LoadOptions, ProgressEvent};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (options, handle) = LoadOptions::new().with_progress();
    
    // 在另一个线程中接收进度
    let progress_thread = std::thread::spawn(move || {
        while let Ok(event) = handle.recv() {
            match event {
                ProgressEvent::Started { total_bytes } => {
                    println!("开始加载，文件大小: {} bytes", total_bytes);
                }
                ProgressEvent::Progress(p) => {
                    print!("\r进度: {:.1}% ({}/{} tracks)", 
                        p.percentage(), 
                        p.tracks_parsed, 
                        p.total_tracks
                    );
                    std::io::Write::flush(&mut std::io::stdout()).unwrap();
                }
                ProgressEvent::TrackComplete { track_index, event_count } => {
                    println!("\n轨道 {} 完成，{} 个事件", track_index, event_count);
                }
                ProgressEvent::Completed => {
                    println!("\n加载完成！");
                    break;
                }
                ProgressEvent::Error(msg) => {
                    eprintln!("\n错误: {}", msg);
                    break;
                }
            }
        }
    });
    
    let midi = MidiLoader::with_options(options).load("song.mid")?;
    
    progress_thread.join().unwrap();
    
    println!("成功加载 {} 个轨道", midi.track_count());
    
    Ok(())
}
```

### 带取消功能的加载

```rust
use lumino_midiloader::{MidiLoader, LoadOptions, ProgressEvent};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let should_cancel = Arc::new(AtomicBool::new(false));
    let should_cancel_clone = should_cancel.clone();
    
    let (options, handle) = LoadOptions::new().with_progress();
    
    // 监听 Ctrl+C
    ctrlc::set_handler(move || {
        println!("\n收到取消信号...");
        should_cancel_clone.store(true, Ordering::SeqCst);
    })?;
    
    // 进度线程
    let progress_thread = std::thread::spawn(move || {
        while let Ok(event) = handle.recv() {
            if should_cancel.load(Ordering::SeqCst) {
                println!("取消加载");
                return None;
            }
            
            match event {
                ProgressEvent::Progress(p) => {
                    println!("进度: {:.1}%", p.percentage());
                }
                ProgressEvent::Completed => break,
                _ => {}
            }
        }
        Some(())
    });
    
    let result = MidiLoader::with_options(options).load("song.mid");
    
    if progress_thread.join().unwrap().is_some() {
        let midi = result?;
        println!("加载成功: {} 轨道", midi.track_count());
    }
    
    Ok(())
}
```

## 事件处理

### 提取所有音符

```rust
use lumino_midiloader::load;

#[derive(Debug)]
struct NoteEvent {
    track: usize,
    tick: u32,
    key: u8,
    velocity: u8,
    is_on: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let midi = load("song.mid")?;
    let mut notes = Vec::new();
    
    for (track_idx, track) in midi.tracks.iter().enumerate() {
        let mut current_tick = 0u32;
        
        for event in &track.events {
            current_tick += event.delta_time;
            
            match &event.kind {
                lumino_midiloader::EventKind::NoteOn(note) => {
                    notes.push(NoteEvent {
                        track: track_idx,
                        tick: current_tick,
                        key: note.key,
                        velocity: note.velocity,
                        is_on: true,
                    });
                }
                lumino_midiloader::EventKind::NoteOff(note) => {
                    notes.push(NoteEvent {
                        track: track_idx,
                        tick: current_tick,
                        key: note.key,
                        velocity: note.velocity,
                        is_on: false,
                    });
                }
                _ => {}
            }
        }
    }
    
    // 按时间排序
    notes.sort_by_key(|n| n.tick);
    
    for note in notes {
        println!("{:?}", note);
    }
    
    Ok(())
}
```

### 提取速度变化

```rust
use lumino_midiloader::load;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let midi = load("song.mid")?;
    
    for (track_idx, track) in midi.tracks.iter().enumerate() {
        let mut current_tick = 0u32;
        
        for event in &track.events {
            current_tick += event.delta_time;
            
            if let lumino_midiloader::EventKind::Meta(meta) = &event.kind {
                if let Some(bpm) = meta.tempo_bpm() {
                    println!("轨道 {} @ tick {}: {:.1} BPM", track_idx, current_tick, bpm);
                }
            }
        }
    }
    
    Ok(())
}
```

### 计算轨道时长

```rust
use lumino_midiloader::load;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let midi = load("song.mid")?;
    
    // 获取时间分割
    let ticks_per_quarter = match midi.header.division {
        lumino_midiloader::Division::TicksPerQuarter(ticks) => ticks as f64,
        _ => {
            println!("SMPTE 时间格式，需要额外计算");
            return Ok(());
        }
    };
    
    // 查找第一个速度事件
    let mut microseconds_per_quarter = 500_000u32; // 默认 120 BPM
    
    for track in &midi.tracks {
        for event in &track.events {
            if let lumino_midiloader::EventKind::Meta(
                lumino_midiloader::MetaEvent::SetTempo(tempo)
            ) = &event.kind {
                microseconds_per_quarter = *tempo;
                break;
            }
        }
    }
    
    let seconds_per_tick = (microseconds_per_quarter as f64 / 1_000_000.0) / ticks_per_quarter;
    
    for (i, track) in midi.tracks.iter().enumerate() {
        let total_ticks = track.total_ticks();
        let duration_seconds = total_ticks as f64 * seconds_per_tick;
        
        println!("轨道 {}: {:.2} 秒 ({} ticks)", 
            i, duration_seconds, total_ticks);
    }
    
    Ok(())
}
```

## 高级用法

### 从内存加载

```rust
use lumino_midiloader::reader::ByteBuffer;
use lumino_midiloader::MidiLoader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 从网络或其他来源获取 MIDI 数据
    let midi_data: Vec<u8> = std::fs::read("song.mid")?;
    
    // 使用 ByteBuffer 从内存读取
    let reader = ByteBuffer::new(&midi_data);
    
    // 注意：当前 API 主要设计为从文件加载
    // 从内存加载需要直接使用 Parser（未来版本可能改进）
    
    println!("MIDI 数据大小: {} bytes", reader.len());
    
    Ok(())
}
```

### 自定义错误处理

```rust
use lumino_midiloader::{load, MidiloaderError};

fn main() {
    match load("song.mid") {
        Ok(midi) => {
            println!("成功加载 {} 个轨道", midi.track_count());
        }
        Err(e) => {
            match &e {
                MidiloaderError::Io(io_err) => {
                    eprintln!("文件错误: {}", io_err);
                }
                MidiloaderError::InvalidHeader(msg) => {
                    eprintln!("无效的 MIDI 文件: {}", msg);
                }
                MidiloaderError::UnsupportedFormat(msg) => {
                    eprintln!("不支持的格式: {}", msg);
                }
                _ if e.is_parse_error() => {
                    eprintln!("解析错误: {}", e);
                }
                _ => {
                    eprintln!("其他错误: {}", e);
                }
            }
        }
    }
}
```

### 批量处理 MIDI 文件

```rust
use lumino_midiloader::load;
use std::path::Path;

fn process_midi_file(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let midi = load(path)?;
    
    println!("文件: {:?}", path);
    println!("  格式: {:?}", midi.header.format);
    println!("  轨道数: {}", midi.track_count());
    println!("  总事件数: {}", midi.total_events());
    
    // 统计音符数量
    let note_count: usize = midi.tracks.iter()
        .map(|t| t.note_on_events().count())
        .sum();
    println!("  音符数: {}", note_count);
    
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let midi_dir = std::path::Path::new("./midi_files");
    
    for entry in std::fs::read_dir(midi_dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.extension().map(|e| e == "mid").unwrap_or(false) {
            if let Err(e) = process_midi_file(&path) {
                eprintln!("处理 {:?} 时出错: {}", path, e);
            }
            println!();
        }
    }
    
    Ok(())
}
```

## 实用工具函数

### 音符名称转换

```rust
fn key_to_name(key: u8) -> String {
    const NOTE_NAMES: &[&str] = &["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    
    let octave = (key / 12) as i8 - 1;
    let note_index = (key % 12) as usize;
    
    format!("{}{}", NOTE_NAMES[note_index], octave)
}

fn main() {
    println!("{} -> {}", 60, key_to_name(60)); // C4
    println!("{} -> {}", 69, key_to_name(69)); // A4
    println!("{} -> {}", 72, key_to_name(72)); // C5
}
```

### 计算 BPM

```rust
fn tempo_to_bpm(microseconds_per_quarter: u32) -> f64 {
    60_000_000.0 / microseconds_per_quarter as f64
}

fn main() {
    // 120 BPM = 500,000 微秒/四分音符
    let tempo = 500_000;
    println!("{} μs/quarter = {:.1} BPM", tempo, tempo_to_bpm(tempo));
}
```
