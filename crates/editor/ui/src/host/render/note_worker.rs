//! 洋葱皮辅助工具 & 预览音符收集
//!
//! 统一全量渲染（2026-08-06）：主音轨可见实例由 GPU 全量 buffer + cull 负责，
//! 本文件仅保留：
//! - `MAIN_TRACK_NOTE_COLOR`：主音轨固定蓝色（与 shader 一致）
//! - `collect_i2m_preview_notes`：图片转 MIDI 预览音符收集

/// 主音轨已放置音符的固定蓝色（与 shader `MAIN_TRACK_COLOR` 一致）
pub(super) const MAIN_TRACK_NOTE_COLOR: [f32; 4] = [0.2, 0.55, 1.0, 1.0];

// ─── 图片转 MIDI 预览音符收集 ─────────────────────────────────────────────

/// 图片转 MIDI 主轨预览音符（tick, key, length）
pub(super) type I2mMainNote = (f32, u8, f32);
/// 图片转 MIDI 洋葱皮预览音符（tick, key, length, 调色板颜色）
pub(super) type I2mOnionNote = (f32, u8, f32, [f32; 4]);

/// 收集图片转 MIDI / 素材预览音符（区域映射后）
///
/// 返回 `(主轨音符, 其他轨洋葱皮音符)`：
/// - 主轨 = `preview.tracks[0]`（颜色 0，插入时写入当前音轨）→ 实色
/// - 其他轨 = `preview.tracks[1..]` → 洋葱皮调色板颜色
///
/// 生效条件：`Placing` 阶段（region 已确认），或素材拖出跟随阶段
/// （`Selecting` + `drag_follow` 存在，预览跟随鼠标移动）。
pub(super) fn collect_i2m_preview_notes(
    editor: &crate::editor::Editor,
) -> (Vec<I2mMainNote>, Vec<I2mOnionNote>) {
    use lumino_editor_state::ImageToMidiMode;

    let i2m = &editor.editor_state.image_to_midi;
    // 素材拖出跟随阶段：Selecting + drag_follow 存在（预览跟随鼠标）
    let material_following = i2m.mode == ImageToMidiMode::Selecting
        && i2m.drag_follow.is_some()
        && i2m.preview.is_some();
    if i2m.mode != ImageToMidiMode::Placing && !material_following {
        return (Vec::new(), Vec::new());
    }
    let Some(preview) = &i2m.preview else {
        return (Vec::new(), Vec::new());
    };

    let mut main_notes = Vec::new();
    let mut onion_notes = Vec::new();
    for (track_idx, _) in preview.tracks.iter().enumerate() {
        let notes = i2m.track_screen_notes(track_idx);
        if track_idx == 0 {
            main_notes.extend(notes);
        } else {
            let color = lumino_extras::palette::current_track_color_f32(track_idx);
            onion_notes.extend(notes.into_iter().map(|(t, k, l)| (t, k, l, color)));
        }
    }
    (main_notes, onion_notes)
}
