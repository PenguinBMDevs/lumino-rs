//! MIDI 编码辅助函数

/// 将 BPM 转换为微秒每拍
pub use lumino_midi_loader::bpm_to_tempo;

/// 将微秒每拍转换为 BPM
pub use lumino_midi_loader::tempo_to_bpm;
