//! 音符纯数据模型

/// 音符逻辑表示（纯数据，不含 UI 相关方法）
#[derive(Debug, Clone)]
pub struct Note {
    pub tick: f32,
    pub key: u16,
    pub length: f32,
    /// 音符力度 (0-127)，默认 100
    pub velocity: u8,
    /// MIDI 通道 (0-15)，默认 0
    pub channel: u8,
}

impl Note {
    pub fn new(tick: f32, key: u16, length: f32) -> Self {
        Self {
            tick,
            key,
            length,
            velocity: 100,
            channel: 0,
        }
    }

    /// 从原始数据元组构造 Note
    pub fn from_raw(tick: f32, key: u16, length: f32, velocity: u8, channel: u8) -> Self {
        Self {
            tick,
            key,
            length,
            velocity,
            channel,
        }
    }

    pub fn with_velocity(mut self, velocity: u8) -> Self {
        self.velocity = velocity;
        self
    }

    pub fn with_channel(mut self, channel: u8) -> Self {
        self.channel = channel;
        self
    }
}
