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
    /// 力度 (0-127)
    pub velocity: u8,
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
    /// 拍号事件列表
    pub time_signatures: Vec<MidiTimeSignatureEvent>,
    /// 调号事件列表
    pub key_signatures: Vec<MidiKeySignatureEvent>,
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
