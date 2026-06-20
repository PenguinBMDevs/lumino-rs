//! 洋葱皮渲染器类型定义

use lumino_core::note::Note;
use lumino_midi_loader::NoteInfo;

/// 键位模式，决定贴图高度
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyMode {
    /// 128 键模式（贴图高度 128 像素）
    Key128,
    /// 256 键模式（贴图高度 256 像素）
    Key256,
}

impl KeyMode {
    pub(crate) fn height(self) -> u32 {
        match self {
            KeyMode::Key128 => 128,
            KeyMode::Key256 => 256,
        }
    }

    pub(crate) fn total_keys(self) -> f32 {
        match self {
            KeyMode::Key128 => 128.0,
            KeyMode::Key256 => 256.0,
        }
    }
}

/// 生成进度信息
#[derive(Debug, Clone, Copy)]
pub struct GenerateProgress {
    /// 已处理的音轨数
    pub processed_tracks: usize,
    /// 总音轨数
    pub total_tracks: usize,
}

impl GenerateProgress {
    /// 计算百分比（0.0 ~ 100.0）
    pub fn percent(self) -> f32 {
        if self.total_tracks == 0 {
            return 100.0;
        }
        (self.processed_tracks as f32 / self.total_tracks as f32) * 100.0
    }
}

/// 视口参数（用于每帧更新 uniform）
#[derive(Debug, Clone, Copy)]
pub struct ViewportParams {
    /// 卷帘区域在 framebuffer 中的 X 位置
    pub area_x: f32,
    /// 卷帘区域在 framebuffer 中的 Y 位置
    pub area_y: f32,
    /// 卷帘区域宽度
    pub area_w: f32,
    /// 卷帘区域高度
    pub area_h: f32,
    /// 当前视口可见的起始时间（毫秒）
    pub time_start_ms: f32,
    /// 当前视口可见的结束时间（毫秒）
    pub time_end_ms: f32,
    /// 当前视口可见的起始键位
    pub key_start: f32,
    /// 当前视口可见的结束键位
    pub key_end: f32,
}

/// 后台生成结果
pub(crate) struct GenerateResult {
    /// RGBA 像素数据（4096 × height × 4 bytes）
    pub pixels: Vec<u8>,
    /// 贴图高度
    pub height: u32,
}

/// 洋葱皮音符数据（用于后台生成）
///
/// 可以是 tick 或毫秒单位，取决于 `generate` 是否传了 tempo_table。
#[derive(Debug, Clone, Copy)]
pub struct OnionSkinNote {
    /// 起始时间（tick 或毫秒）
    pub start_tick: u32,
    /// 结束时间（tick 或毫秒）
    pub end_tick: u32,
    /// 起始毫秒（如果已经是毫秒单位）
    pub start_ms: f32,
    /// 结束毫秒
    pub end_ms: f32,
    /// MIDI key (0-127)
    pub key: u8,
    /// RGBA 颜色
    pub color: [u8; 4],
}

impl OnionSkinNote {
    /// 从 NoteInfo 创建（tick 单位）
    pub fn from_note_info(note: &NoteInfo, color: [u8; 4]) -> Self {
        Self {
            start_tick: note.start_tick,
            end_tick: note.end_tick(),
            start_ms: 0.0,
            end_ms: 0.0,
            key: note.key,
            color,
        }
    }

    /// 从 Note 创建（tick 单位）
    pub fn from_note(note: &Note, color: [u8; 4]) -> Self {
        Self {
            start_tick: note.tick as u32,
            end_tick: (note.tick + note.length) as u32,
            start_ms: 0.0,
            end_ms: 0.0,
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

    #[test]
    fn test_key_mode_height() {
        assert_eq!(KeyMode::Key128.height(), 128);
        assert_eq!(KeyMode::Key256.height(), 256);
    }

    #[test]
    fn test_key_mode_total_keys() {
        assert!((KeyMode::Key128.total_keys() - 128.0).abs() < f32::EPSILON);
        assert!((KeyMode::Key256.total_keys() - 256.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_generate_progress_percent() {
        let p = GenerateProgress {
            processed_tracks: 3,
            total_tracks: 10,
        };
        assert!((p.percent() - 30.0).abs() < 0.001);

        let p = GenerateProgress {
            processed_tracks: 0,
            total_tracks: 0,
        };
        assert!((p.percent() - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_onion_skin_note_creation() {
        let note_info = NoteInfo::new(100, 200, 60, 100, 0);
        let skin_note = OnionSkinNote::from_note_info(&note_info, [255, 0, 0, 255]);
        assert_eq!(skin_note.start_tick, 100);
        assert_eq!(skin_note.end_tick, 300);
        assert_eq!(skin_note.key, 60);
        assert_eq!(skin_note.color, [255, 0, 0, 255]);
    }
}
