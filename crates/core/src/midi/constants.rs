/// 默认的 PPQN (Pulses Per Quarter Note) 值
pub const DEFAULT_PPQN: u16 = 1920;

/// MIDI 标准通道数
pub const MIDI_CHANNEL_COUNT: u8 = 16;

/// 扩展 MIDI Key 范围 (0-255)
pub const MIDI_KEY_RANGE: u16 = 256;

/// 最大并发音符数 (256 keys × 16 channels)
pub const MAX_CONCURRENT_NOTES: usize = 256 * 16;

/// Tick 搜索缓冲区大小
pub const TICK_SEARCH_BUFFER: u32 = 19200;

/// 默认 BPM
pub const DEFAULT_BPM: f64 = 120.0;

/// 默认 tempo (微秒/拍)，对应 120 BPM
pub const DEFAULT_TEMPO_MICROS: u32 = 500_000;
