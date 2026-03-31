//! DMS 导出类型定义

use crate::error::ExportResult;

/// DMS 导出选项
#[derive(Debug, Clone, Default)]
pub struct DmsExportOptions {
    /// 歌曲名称
    pub song_name: Option<String>,
    /// 版权信息
    pub copyright: Option<String>,
    /// 歌曲备注
    pub comment: Option<String>,
    /// PPQN (每四分音符脉冲数)
    pub ppqn: Option<u32>,
}

/// DMS 音符事件
#[derive(Debug, Clone)]
pub struct DmsNoteEvent {
    /// Tick 位置
    pub tick: u64,
    /// 键号 (0-127)
    pub key: u8,
    /// 力度 (0-127)
    pub velocity: u8,
    /// 门限 (tick)
    pub gate: u64,
}

/// DMS 速度事件
#[derive(Debug, Clone)]
pub struct DmsTempoEvent {
    /// Tick 位置
    pub tick: u64,
    /// 速度值 (BPM)
    pub tempo: f64,
}

/// DMS 控制事件
#[derive(Debug, Clone)]
pub struct DmsControlEvent {
    /// Tick 位置
    pub tick: u64,
    /// 控制类型 (CC 编号)
    pub control_type: u8,
    /// 控制值
    pub value: f64,
    /// 门限
    pub gate: f64,
}

/// DMS 轨道
#[derive(Debug, Clone)]
pub struct DmsTrack {
    /// 轨道名称
    pub name: Option<String>,
    /// 端口 (0-15)
    pub port: u8,
    /// 通道 (0-15)
    pub channel: u8,
    /// 是否为鼓轨道
    pub is_drum: bool,
    /// 音符事件列表
    pub notes: Vec<DmsNoteEvent>,
    /// 速度事件列表
    pub tempos: Vec<DmsTempoEvent>,
    /// 控制事件列表
    pub controls: Vec<DmsControlEvent>,
}

/// DMS 导出数据
#[derive(Debug, Clone)]
pub struct DmsExportData {
    /// 导出选项
    pub options: DmsExportOptions,
    /// 轨道列表
    pub tracks: Vec<DmsTrack>,
}

/// DMS 常量
pub mod constants {
    /// 最大音符值
    pub const MAX_NOTE_VALUE: u16 = 65535;
    /// 最小音符值
    pub const MIN_NOTE_VALUE: u16 = 0;
}
