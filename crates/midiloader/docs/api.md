# API 文档

## 核心类型

### `MidiFile`

MIDI 文件结构，包含文件头和轨道列表。

```rust
pub struct MidiFile {
    pub header: Header,
    pub tracks: Vec<Track>,
}
```

#### 方法

- `track_count()` - 获取轨道数量
- `total_events()` - 获取所有事件的总数
- `find_track_by_name(name: &str)` - 查找指定名称的轨道

### `Header`

MIDI 文件头信息。

```rust
pub struct Header {
    pub format: Format,      // 文件格式
    pub ntracks: u16,        // 轨道数量
    pub division: Division,  // 时间分割方式
}
```

### `Format`

MIDI 文件格式类型。

```rust
pub enum Format {
    SingleTrack,           // 格式 0：单轨道
    MultiTrackSync,        // 格式 1：多轨道同步
    MultiTrackIndependent, // 格式 2：多轨道独立
}
```

### `Track`

MIDI 轨道。

```rust
pub struct Track {
    pub name: Option<String>,  // 轨道名称
    pub events: Vec<Event>,    // 事件列表
}
```

#### 方法

- `new()` - 创建空轨道
- `push(event: Event)` - 添加事件
- `len()` - 获取事件数量
- `is_empty()` - 检查是否为空
- `note_on_events()` - 获取所有 Note On 事件
- `total_ticks()` - 计算轨道总时长（tick）

### `Event`

MIDI 事件。

```rust
pub struct Event {
    pub delta_time: u32,       // 距离上一个事件的 tick 数
    pub kind: EventKind,       // 事件类型
    pub channel: Option<u8>,   // 通道号（系统事件为 None）
}
```

#### 方法

- `new(delta_time, kind, channel)` - 创建新事件
- `is_note()` - 检查是否为音符事件
- `is_meta()` - 检查是否为元事件

## 便捷函数

### `load`

加载 MIDI 文件的便捷函数。

```rust
pub fn load<P: AsRef<Path>>(path: P) -> Result<MidiFile>
```

**示例：**

```rust
use lumino_midiloader::load;

let midi = load("song.mid")?;
println!("Loaded {} tracks", midi.track_count());
```

### `load_with_options`

使用指定选项加载 MIDI 文件。

```rust
pub fn load_with_options<P: AsRef<Path>>(
    path: P,
    options: LoadOptions
) -> Result<MidiFile>
```

## MidiLoader

MIDI 文件加载器，提供更高级的控制。

### 方法

#### `new()`

创建默认加载器。

```rust
let loader = MidiLoader::new();
let midi = loader.load("song.mid")?;
```

#### `with_options(options: LoadOptions)`

使用指定选项创建加载器。

```rust
let options = LoadOptions::new();
let loader = MidiLoader::with_options(options);
```

#### `set_reporter(reporter: ProgressReporter)`

设置进度报告器。

```rust
let mut loader = MidiLoader::new();
loader.set_reporter(reporter);
```

#### `load<P: AsRef<Path>>(self, path: P) -> Result<MidiFile>`

加载 MIDI 文件。

## LoadOptions

加载选项。

```rust
pub struct LoadOptions {
    pub report_progress: bool,        // 是否启用进度报告
    pub progress_channel_capacity: usize,  // 进度报告通道容量
}
```

### 方法

#### `new()`

创建默认选项。

```rust
let options = LoadOptions::new();
```

#### `with_progress()`

启用进度报告，返回选项和进度句柄。

```rust
let (options, handle) = LoadOptions::new().with_progress();

// 在另一个线程中接收进度
std::thread::spawn(move || {
    while let Ok(event) = handle.recv() {
        match event {
            ProgressEvent::Started { total_bytes } => {
                println!("开始加载，总字节数: {}", total_bytes);
            }
            ProgressEvent::Progress(p) => {
                println!("进度: {:.1}%", p.percentage());
            }
            ProgressEvent::Completed => {
                println!("加载完成");
                break;
            }
            _ => {}
        }
    }
});

let midi = MidiLoader::with_options(options).load("song.mid")?;
```

#### `with_channel_capacity(capacity: usize)`

设置进度报告通道容量。

```rust
let options = LoadOptions::new()
    .with_channel_capacity(2048);
```

## 进度报告

### `Progress`

进度信息。

```rust
pub struct Progress {
    pub bytes_read: u64,       // 已读取的字节数
    pub total_bytes: u64,      // 总字节数
    pub events_parsed: u64,    // 已解析的事件数
    pub tracks_parsed: u16,    // 已解析的轨道数
    pub total_tracks: u16,     // 总轨道数
}
```

#### 方法

- `percentage()` - 计算完成百分比
- `is_complete()` - 检查是否已完成
- `track_percentage()` - 计算轨道完成百分比

### `ProgressEvent`

进度事件类型。

```rust
pub enum ProgressEvent {
    Started { total_bytes: u64 },
    Progress(Progress),
    TrackComplete { track_index: u16, event_count: u64 },
    Completed,
    Error(String),
}
```

### `ProgressHandle`

进度句柄，用于接收进度事件。

#### 方法

- `new()` - 创建新的进度句柄和报告器
- `receiver()` - 获取接收器的引用
- `try_recv()` - 尝试接收事件（非阻塞）
- `recv()` - 接收事件（阻塞）

## 错误处理

### `MidiloaderError`

错误类型。

```rust
pub enum MidiloaderError {
    Io(std::io::Error),
    Mmap(String),
    InvalidHeader(String),
    InvalidTrackData(String),
    InvalidEventData(String),
    UnsupportedFormat(String),
    InvalidUtf8(std::string::FromUtf8Error),
    UnexpectedEof,
    InvalidVarLen,
    TrackIndexOutOfRange { index: usize, max: usize },
    InvalidChannel { channel: u8 },
    InvalidKey { key: u8 },
}
```

### 便捷方法

- `invalid_header(msg)` - 创建无效的 MIDI 文件头错误
- `invalid_track_data(msg)` - 创建无效的轨道数据错误
- `invalid_event_data(msg)` - 创建无效的事件数据错误
- `unsupported_format(msg)` - 创建不支持的格式错误
- `is_io_error()` - 检查是否为 IO 错误
- `is_parse_error()` - 检查是否为解析错误

## 读取器

### `BinaryReader` Trait

二进制数据读取 trait。

```rust
pub trait BinaryReader {
    fn position(&self) -> usize;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
    fn remaining(&self) -> usize;
    fn seek(&mut self, position: usize);
    fn skip(&mut self, count: usize);
    fn peek(&self, count: usize) -> Option<&[u8]>;
    fn read(&mut self, count: usize) -> Option<&[u8]>;
    fn read_u8(&mut self) -> Option<u8>;
    fn read_u16_be(&mut self) -> Option<u16>;
    fn read_u32_be(&mut self) -> Option<u32>;
    fn read_varlen(&mut self) -> Option<u32>;
}
```

### `MmapReader`

基于内存映射文件的读取器。

```rust
pub struct MmapReader { /* ... */ }
```

#### 方法

- `open(path)` - 打开文件并创建读取器
- `slice(start, end)` - 获取指定范围的切片
- `current_slice(count)` - 获取从当前位置开始的切片

### `ByteBuffer`

基于字节切片的读取器。

```rust
pub struct ByteBuffer<'a> { /* ... */ }
```

#### 方法

- `new(data)` - 从字节切片创建读取器
