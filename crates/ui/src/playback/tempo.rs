//! 速度（Tempo）管理模块
//!
//! 负责BPM和tempo（微秒/四分音符）之间的转换

/// 速度变化事件
#[derive(Debug, Clone)]
pub struct TempoChange {
    /// 发生时刻（tick）
    pub tick: f32,
    /// 速度值（微秒/四分音符）
    pub tempo: u32,
}

impl TempoChange {
    /// 从速度值创建（微秒/四分音符）
    pub fn from_tempo(tick: f32, tempo: u32) -> Self {
        Self { tick, tempo }
    }

    /// 从BPM创建
    pub fn from_bpm(tick: f32, bpm: f64) -> Self {
        let tempo = tempo_from_bpm(bpm);
        Self { tick, tempo }
    }

    /// 获取BPM值
    pub fn bpm(&self) -> f64 {
        bpm_from_tempo(self.tempo)
    }
}

/// 将BPM转换为tempo（微秒/四分音符）
pub use lumino_midi_loader::bpm_to_tempo as tempo_from_bpm;

/// 将tempo（微秒/四分音符）转换为BPM
pub use lumino_midi_loader::tempo_to_bpm as bpm_from_tempo;
