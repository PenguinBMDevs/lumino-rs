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

/// 将 MIDI 音高数字转换为音名（升号表示法）。
///
/// 例如 `63` → `"D#"`，`64` → `"E"`。八度按 MIDI 标准计算：
/// C4 = 60，每 12 一个八度。
pub fn note_name(key: u16) -> String {
    const NAMES: &[&str] = &[
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let octave = (key as i32 / 12) - 1;
    let name = NAMES[(key as usize) % 12];
    format!("{}{}", name, octave)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_note_new_defaults() {
        let note = Note::new(100.0, 60, 480.0);
        assert_eq!(note.tick, 100.0);
        assert_eq!(note.key, 60);
        assert_eq!(note.length, 480.0);
        assert_eq!(note.velocity, 100); // 默认力度
        assert_eq!(note.channel, 0); // 默认通道
    }

    #[test]
    fn test_note_from_raw() {
        let note = Note::from_raw(200.0, 72, 960.0, 127, 5);
        assert_eq!(note.tick, 200.0);
        assert_eq!(note.key, 72);
        assert_eq!(note.length, 960.0);
        assert_eq!(note.velocity, 127);
        assert_eq!(note.channel, 5);
    }

    #[test]
    fn test_note_with_velocity() {
        let note = Note::new(0.0, 60, 480.0).with_velocity(80);
        assert_eq!(note.velocity, 80);
    }

    #[test]
    fn test_note_with_channel() {
        let note = Note::new(0.0, 60, 480.0).with_channel(10);
        assert_eq!(note.channel, 10);
    }

    #[test]
    fn test_note_builder_chain() {
        let note = Note::new(10.0, 64, 240.0).with_velocity(90).with_channel(3);
        assert_eq!(note.tick, 10.0);
        assert_eq!(note.key, 64);
        assert_eq!(note.length, 240.0);
        assert_eq!(note.velocity, 90);
        assert_eq!(note.channel, 3);
    }

    #[test]
    fn test_note_clone() {
        let n1 = Note::new(100.0, 60, 480.0);
        let n2 = n1.clone();
        assert_eq!(n1.tick, n2.tick);
        assert_eq!(n1.key, n2.key);
        assert_eq!(n1.length, n2.length);
    }

    #[test]
    fn test_note_velocity_range() {
        let note = Note::new(0.0, 60, 480.0).with_velocity(0);
        assert_eq!(note.velocity, 0);
        let note = Note::new(0.0, 60, 480.0).with_velocity(127);
        assert_eq!(note.velocity, 127);
    }

    #[test]
    fn test_note_name() {
        assert_eq!(note_name(60), "C4");
        assert_eq!(note_name(63), "D#4");
        assert_eq!(note_name(64), "E4");
        assert_eq!(note_name(0), "C-1");
        assert_eq!(note_name(127), "G9");
    }
}
