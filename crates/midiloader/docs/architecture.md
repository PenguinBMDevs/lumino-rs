# 架构设计

## 概述

Lumino MIDI Loader 采用分层架构设计，将功能划分为清晰的模块，便于维护和扩展。

## 模块结构

```
midiloader/
├── src/
│   ├── lib.rs          # 库入口，导出公共 API
│   ├── error.rs        # 错误类型定义
│   ├── model.rs        # 数据模型（MIDI 结构）
│   ├── parser.rs       # MIDI 解析器
│   ├── progress.rs     # 进度报告
│   └── reader.rs       # 二进制读取器
```

## 核心组件

### 1. Reader 模块

负责底层二进制数据的读取。

#### BinaryReader Trait

定义了通用的二进制读取接口：

- 基本操作：`position()`, `len()`, `remaining()`
- 导航操作：`seek()`, `skip()`
- 数据读取：`read()`, `peek()`
- 类型读取：`read_u8()`, `read_u16_be()`, `read_u32_be()`, `read_varlen()`

#### MmapReader

使用内存映射文件实现高效的大文件读取：

- 优点：
  - 操作系统自动管理内存
  - 延迟加载，只读取需要的部分
  - 多个进程可以共享同一文件的内存映射

- 实现细节：
  - 使用 `memmap2` crate 进行跨平台内存映射
  - 维护当前位置指针
  - 提供安全的边界检查

#### ByteBuffer

用于从内存中的字节切片读取数据：

- 用途：
  - 测试
  - 从内存中加载 MIDI 数据
  - 嵌入式场景

### 2. Model 模块

定义 MIDI 文件的数据结构。

#### 核心类型

- `MidiFile`: 顶层结构，包含 header 和 tracks
- `Header`: 文件头信息（格式、轨道数、时间分割）
- `Track`: 轨道，包含事件列表
- `Event`: 事件，包含 delta_time、kind 和 channel

#### 事件类型

- 通道事件：`NoteOn`, `NoteOff`, `CC`, `ProgramChange`, `PitchBend` 等
- 元事件：`SetTempo`, `TimeSignature`, `KeySignature`, `TrackName` 等
- 系统独占事件：`SysEx`

### 3. Parser 模块

实现 MIDI 文件的解析逻辑。

#### 解析流程

```
1. 打开文件 -> MmapReader
2. 解析 Header
   - 验证 "MThd" 标记
   - 读取格式、轨道数、时间分割
3. 解析 Tracks
   - 对每个轨道：
     - 验证 "MTrk" 标记
     - 读取轨道长度
     - 解析事件直到轨道结束
4. 返回 MidiFile
```

#### 事件解析

- 读取 delta time（变长数值）
- 读取状态字节
- 根据状态字节解析事件数据
- 处理 Running Status 优化

#### 变长数值（Variable Length Quantity）

MIDI 使用变长数值编码来节省空间：

- 每个字节使用 7 位存储数据
- 最高位为 1 表示还有后续字节
- 最高位为 0 表示数值结束
- 最大 4 个字节，可表示 0 到 2^28-1

### 4. Progress 模块

实现进度报告功能。

#### 设计

使用多生产者单消费者（MPSC）通道：

- `ProgressReporter`: 生产者，在解析器中报告进度
- `ProgressHandle`: 消费者，供用户接收进度事件
- `ProgressEvent`: 事件类型（Started, Progress, Completed, Error）

#### 线程安全

- 使用 `crossbeam-channel` 实现无锁通道
- `ProgressReporter` 实现了 `Clone`，可在多个地方使用
- 通道有界，防止内存无限增长

### 5. Error 模块

定义错误类型和处理。

#### 错误分类

- IO 错误：文件操作失败
- 解析错误：数据格式不正确
- 验证错误：数据不符合 MIDI 规范

#### 错误处理策略

- 使用 `thiserror` 派生 `Error` trait
- 提供详细的错误信息
- 支持错误链（通过 `#[from]`）

## 关键设计决策

### 1. 内存映射 vs 流式读取

**选择：内存映射**

原因：
- MIDI 文件通常不大（几 MB）
- 随机访问需求（解析事件时需要跳转）
- 代码更简单，不需要复杂的缓冲管理

### 2. 所有权模型

- `MidiFile` 拥有所有数据
- 读取器在解析完成后丢弃
- 使用 `Vec<u8>` 存储变长数据（如 SysEx）

### 3. 零拷贝优化

- 使用内存映射避免文件到内存的拷贝
- 字符串数据需要拷贝（UTF-8 验证）
- 事件数据需要拷贝（构造 Vec）

### 4. 进度报告设计

- 可选功能，不使用时无开销
- 异步报告，不阻塞解析
- 用户可以自定义处理进度事件

## 性能考虑

### 优化点

1. **预分配容量**
   - `Vec::with_capacity(header.ntracks as usize)`
   - 避免解析过程中的多次内存分配

2. **避免不必要的拷贝**
   - 使用内存映射直接访问文件数据
   - 仅在必要时进行数据转换

3. **批量处理**
   - 一次读取多个字节
   - 减少函数调用开销

### 潜在优化

1. **并行解析**
   - 格式 1 的多个轨道可以并行解析
   - 需要权衡并行开销

2. **延迟解析**
   - 只在需要时解析事件
   - 适用于只需要元数据的场景

3. **缓存**
   - 缓存最近解析的文件
   - 适用于重复加载相同文件的场景

## 扩展性

### 添加新的事件类型

1. 在 `EventKind` 枚举中添加新变体
2. 在 `Parser` 中添加解析逻辑
3. 更新文档和测试

### 支持新的文件格式

1. 实现新的解析器（如 `XmfParser`）
2. 使用 trait 抽象通用接口
3. 在 `MidiLoader` 中添加格式检测

### 自定义读取器

实现 `BinaryReader` trait：

```rust
impl BinaryReader for MyReader {
    // 实现必要的方法
}
```

## 测试策略

### 单元测试

- 每个模块独立的单元测试
- 覆盖正常和异常路径
- 使用内存缓冲区避免文件 IO

### 集成测试

- 使用真实 MIDI 文件测试
- 测试各种格式和边缘情况
- 性能基准测试

### 模糊测试

- 使用随机数据测试解析器的健壮性
- 确保不会因无效输入而 panic
