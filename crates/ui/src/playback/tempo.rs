//! 速度（Tempo）管理模块

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
pub fn tempo_from_bpm(bpm: f64) -> u32 {
    (60_000_000.0 / bpm).round() as u32
}

/// 将tempo（微秒/四分音符）转换为BPM
pub fn bpm_from_tempo(tempo: u32) -> f64 {
    60_000_000.0 / tempo as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tempo_conversion() {
        let bpm = 120.0;
        let tempo = tempo_from_bpm(bpm);
        assert_eq!(tempo, 500_000);
        assert_eq!(bpm_from_tempo(tempo), bpm);
    }
}
