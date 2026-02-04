# Lumino MIDI Loader 文档

## 目录

- [API 文档](api.md) - 详细的 API 使用说明
- [架构设计](architecture.md) - 内部架构和设计理念
- [MIDI 格式支持](midi-format.md) - 支持的 MIDI 格式详解
- [性能优化](performance.md) - 性能优化建议
- [示例代码](examples.md) - 使用示例

## 快速开始

```rust
use lumino_midiloader::load;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 加载 MIDI 文件
    let midi = load("path/to/song.mid")?;
    
    println!("格式: {}", midi.header.format);
    println!("轨道数: {}", midi.track_count());
    println!("总事件数: {}", midi.total_events());
    
    Ok(())
}
```

## 特性

- 支持 MIDI 格式 0, 1, 2
- 完整的 MIDI 事件解析
- 内存映射文件读取（高性能）
- 进度报告功能
- 零拷贝读取（尽可能）
- 全面的错误处理
