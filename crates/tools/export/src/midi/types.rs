//! MIDI 导出类型定义
//!
//! 本模块定义了 MIDI 导出所需的所有数据结构，包括音符事件、控制器事件、
//! 轨道数据和导出选项等。与导出逻辑分离，便于独立测试和复用。

/// MIDI 导出选项
#[derive(Debug, Clone, Default)]
pub struct MidiExportOptions {
    /// MIDI 格式 (0 = 单轨道, 1 = 多轨道同步)
    pub format: u16,
    /// PPQN (每四分音符脉冲数)
    pub ppqn: u16,
}

/// MIDI 音符事件
#[derive(Debug, Clone)]
pub struct MidiNoteEvent {
    /// Tick 位置
    pub tick: u32,
    /// 通道 (0-15)
    pub channel: u8,
    /// 键号 (0-127)
    pub key: u8,
    /// 按压力度 (0-127)
    pub velocity: u8,
    /// 释放力度 / NoteOff velocity (0-127)，缺失时为 0
    pub release_velocity: u8,
    /// 持续时间 (tick)
    pub duration: u32,
}

/// MIDI 速度事件
#[derive(Debug, Clone)]
pub struct MidiTempoEvent {
    /// Tick 位置
    pub tick: u32,
    /// 速度值 (微秒每拍)
    pub tempo: u32,
}

/// MIDI 程序变更事件
#[derive(Debug, Clone)]
pub struct MidiProgramChangeEvent {
    /// Tick 位置
    pub tick: u32,
    /// 通道 (0-15)
    pub channel: u8,
    /// 程序号 (0-127)
    pub program: u8,
}

/// MIDI 控制变更事件
#[derive(Debug, Clone)]
pub struct MidiControlChangeEvent {
    /// Tick 位置
    pub tick: u32,
    /// 通道 (0-15)
    pub channel: u8,
    /// 控制器号 (0-127)
    pub controller: u8,
    /// 控制值 (0-127)
    pub value: u8,
}

/// MIDI 拍号事件
#[derive(Debug, Clone)]
pub struct MidiTimeSignatureEvent {
    /// Tick 位置
    pub tick: u32,
    /// 分子
    pub numerator: u8,
    /// 分母 (2 的幂次)
    pub denominator: u8,
    /// 每拍的时钟数
    pub clocks_per_tick: u8,
    /// 32分音符数
    pub notated_32nd_notes_per_beat: u8,
}

/// MIDI 弯音事件
#[derive(Debug, Clone)]
pub struct MidiPitchBendEvent {
    /// Tick 位置
    pub tick: u32,
    /// 通道 (0-15)
    pub channel: u8,
    /// 14-bit 弯音值 (0..16383, 8192 = 中心)
    pub value: u16,
}

/// MIDI 调号事件
#[derive(Debug, Clone)]
pub struct MidiKeySignatureEvent {
    /// Tick 位置
    pub tick: u32,
    /// 调号 (-7 到 7)
    pub key: i8,
    /// 是否为大调
    pub is_major: bool,
}

/// MIDI 通道触后事件
#[derive(Debug, Clone)]
pub struct MidiChannelAftertouchEvent {
    /// Tick 位置
    pub tick: u32,
    /// 通道 (0-15)
    pub channel: u8,
    /// 压力值 (0-127)
    pub velocity: u8,
}

/// MIDI 复音触后事件
#[derive(Debug, Clone)]
pub struct MidiPolyAftertouchEvent {
    /// Tick 位置
    pub tick: u32,
    /// 通道 (0-15)
    pub channel: u8,
    /// 键号 (0-127)
    pub key: u8,
    /// 压力值 (0-127)
    pub velocity: u8,
}

/// MIDI 歌词事件（payload 存原始字节，写盘时原样回写）
#[derive(Debug, Clone)]
pub struct MidiLyricEvent {
    /// Tick 位置
    pub tick: u32,
    /// 原始字节
    pub bytes: Vec<u8>,
}

/// MIDI 标记事件（payload 存原始字节，写盘时原样回写）
#[derive(Debug, Clone)]
pub struct MidiMarkerEvent {
    /// Tick 位置
    pub tick: u32,
    /// 原始字节
    pub bytes: Vec<u8>,
}

/// MIDI 文本类元事件（0x01/0x02/0x04/0x07/0x08/0x09，payload 存原始字节）
#[derive(Debug, Clone)]
pub struct MidiTextMetaEvent {
    /// Tick 位置
    pub tick: u32,
    /// 元事件类型字节
    pub meta_type: u8,
    /// 原始字节
    pub bytes: Vec<u8>,
}

/// MIDI SysEx 事件（payload 存原始字节，写盘时原样回写）
#[derive(Debug, Clone)]
pub struct MidiSysExEvent {
    /// Tick 位置
    pub tick: u32,
    /// 原始字节
    pub bytes: Vec<u8>,
}

/// MIDI 音轨
#[derive(Debug, Clone, Default)]
pub struct MidiTrackData {
    /// 音符事件列表
    pub notes: Vec<MidiNoteEvent>,
    /// 速度事件列表 (通常放在第一个轨道)
    pub tempos: Vec<MidiTempoEvent>,
    /// 程序变更事件列表
    pub program_changes: Vec<MidiProgramChangeEvent>,
    /// 控制变更事件列表
    pub control_changes: Vec<MidiControlChangeEvent>,
    /// 弯音事件列表
    pub pitch_bends: Vec<MidiPitchBendEvent>,
    /// 通道触后事件列表
    pub channel_aftertouch: Vec<MidiChannelAftertouchEvent>,
    /// 复音触后事件列表
    pub poly_aftertouch: Vec<MidiPolyAftertouchEvent>,
    /// 拍号事件列表
    pub time_signatures: Vec<MidiTimeSignatureEvent>,
    /// 调号事件列表
    pub key_signatures: Vec<MidiKeySignatureEvent>,
    /// 歌词事件列表
    pub lyrics: Vec<MidiLyricEvent>,
    /// 标记事件列表
    pub markers: Vec<MidiMarkerEvent>,
    /// 文本类元事件列表
    pub text_events: Vec<MidiTextMetaEvent>,
    /// SysEx 事件列表
    pub sys_ex: Vec<MidiSysExEvent>,
    /// MIDI 端口（FF 21），None 表示不写
    pub midi_port: Option<u8>,
    /// 轨道名称
    pub name: Option<String>,
}

/// MIDI 导出数据
#[derive(Debug, Clone)]
pub struct MidiExportData {
    /// 导出选项
    pub options: MidiExportOptions,
    /// 轨道列表
    pub tracks: Vec<MidiTrackData>,
}
