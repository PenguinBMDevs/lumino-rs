//! MIDI 速度转换工具

/// 将 BPM 转换为微秒每拍
#[inline]
pub fn bpm_to_tempo(bpm: f64) -> u32 {
    let tempo = 60_000_000.0 / bpm;
    tempo.round() as u32
}

/// 将微秒每拍转换为 BPM
#[inline]
pub fn tempo_to_bpm(tempo: u32) -> f64 {
    60_000_000.0 / tempo as f64
}
