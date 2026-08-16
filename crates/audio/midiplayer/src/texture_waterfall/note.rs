//! 贴图瀑布流类型定义

use lumino_midi_loader::NoteInfo;
use lumino_note_core::Note;

/// 贴图瀑布流音符数据（用于后台生成）
///
/// 可以是 tick 或毫秒单位，取决于 `generate` 是否传了 tempo_table。
#[derive(Debug, Clone, Copy)]
pub struct WaterfallNote {
    /// 起始时间（tick 或毫秒）
    pub start_tick: u32,
    /// 结束时间（tick 或毫秒）
    pub end_tick: u32,
    /// 起始毫秒（如果已经是毫秒单位）
    pub start_ms: f32,
    /// 结束毫秒
    pub end_ms: f32,
    /// MIDI key (0-255，支持 256 键，与 NoteInstance u8 编码一致)
    pub key: u8,
    /// RGBA 颜色
    pub color: [u8; 4],
}

impl WaterfallNote {
    /// 从 NoteInfo 创建（tick 单位）
    ///
    /// 注意：`generate_waterfall_track_tile` 以 `start_ms`/`end_ms` 字段作为 tick 值筛选，
    /// 因此 tick 单位的构造器需将 tick 同步写入这两个字段。
    pub fn from_note_info(note: &NoteInfo, color: [u8; 4]) -> Self {
        let start_tick = note.start_tick;
        let end_tick = note.end_tick();
        Self {
            start_tick,
            end_tick,
            start_ms: start_tick as f32,
            end_ms: end_tick as f32,
            key: note.key,
            color,
        }
    }

    /// 从 NoteEvent 创建（tick 单位）
    ///
    /// 注意：`generate_waterfall_track_tile` 以 `start_ms`/`end_ms` 字段作为 tick 值筛选，
    /// 因此 tick 单位的构造器需将 tick 同步写入这两个字段。
    pub fn from_note_event(note: &lumino_midi_loader::NoteEvent, color: [u8; 4]) -> Self {
        let start_tick = note.start_tick;
        let end_tick = note.end_tick();
        Self {
            start_tick,
            end_tick,
            start_ms: start_tick as f32,
            end_ms: end_tick as f32,
            key: note.key,
            color,
        }
    }

    /// 从 Note 创建（tick 单位）
    ///
    /// 注意：`generate_waterfall_track_tile` 以 `start_ms`/`end_ms` 字段作为 tick 值筛选，
    /// 因此 tick 单位的构造器需将 tick 同步写入这两个字段。
    pub fn from_note(note: &Note, color: [u8; 4]) -> Self {
        let start_tick = note.tick as u32;
        let end_tick = (note.tick + note.length) as u32;
        Self {
            start_tick,
            end_tick,
            start_ms: start_tick as f32,
            end_ms: end_tick as f32,
            key: note.key as u8,
            color,
        }
    }

    /// 从毫秒数据创建
    pub fn from_ms(start_ms: f32, end_ms: f32, key: u8, color: [u8; 4]) -> Self {
        Self {
            start_tick: 0,
            end_tick: 0,
            start_ms,
            end_ms,
            key,
            color,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_midi_loader::NoteEvent;
    use lumino_midi_loader::NoteInfo;
    use lumino_note_core::Note;

    #[test]
    fn from_ms_sets_tick_to_zero() {
        let color = [255, 0, 0, 255];
        let note = WaterfallNote::from_ms(100.0, 200.0, 60, color);
        assert_eq!(note.start_tick, 0);
        assert_eq!(note.end_tick, 0);
        assert_eq!(note.start_ms, 100.0);
        assert_eq!(note.end_ms, 200.0);
        assert_eq!(note.key, 60);
        assert_eq!(note.color, color);
    }

    #[test]
    fn from_note_converts_tick_to_float() {
        let color = [0, 255, 0, 255];
        let note = Note::from_raw(480.0, 60, 240.0, 100, 0);
        let skin = WaterfallNote::from_note(&note, color);
        assert_eq!(skin.start_tick, 480);
        assert_eq!(skin.end_tick, 720); // 480 + 240
        assert_eq!(skin.start_ms, 480.0);
        assert_eq!(skin.end_ms, 720.0);
        assert_eq!(skin.key, 60);
        assert_eq!(skin.color, color);
    }

    #[test]
    fn from_note_info_copies_tick_fields() {
        let color = [0, 0, 255, 255];
        let info = NoteInfo::new(960, 480, 72, 80, 1);
        let skin = WaterfallNote::from_note_info(&info, color);
        assert_eq!(skin.start_tick, 960);
        assert_eq!(skin.end_tick, 1440); // 960 + 480
        assert_eq!(skin.start_ms, 960.0);
        assert_eq!(skin.end_ms, 1440.0);
        assert_eq!(skin.key, 72);
        assert_eq!(skin.color, color);
    }

    #[test]
    fn from_note_event_copies_tick_fields() {
        let color = [255, 255, 0, 255];
        let event = NoteEvent::new(1920, 2880, 48, 120, 0);
        let skin = WaterfallNote::from_note_event(&event, color);
        assert_eq!(skin.start_tick, 1920);
        assert_eq!(skin.end_tick, 2880);
        assert_eq!(skin.start_ms, 1920.0);
        assert_eq!(skin.end_ms, 2880.0);
        assert_eq!(skin.key, 48);
        assert_eq!(skin.color, color);
    }

    #[test]
    fn from_ms_and_from_note_produce_compatible_tick_and_ms() {
        // 验证 tick 构造器的 start_ms/end_ms 与 tick 字段一致
        let note = Note::from_raw(100.0, 60, 50.0, 100, 0);
        let skin = WaterfallNote::from_note(&note, [0, 0, 0, 255]);
        assert_eq!(skin.start_ms, skin.start_tick as f32);
        assert_eq!(skin.end_ms, skin.end_tick as f32);
    }
}
