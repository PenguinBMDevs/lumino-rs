//! Mask 预览层实例：ghost 压暗 + 可见片段回叠 + silent mask 虚线轮廓
//!
//! 预览层由 `NoteRenderer` 在主音符之后绘制（深度 0.0），所以这些实例是真正的
//! 覆盖层；无遮罩时返回空，原画面完全恢复。

use lumino_gfx::NoteInstance;

/// 构建当前工程的 mask 预览实例
pub(super) fn instances(editor: &lumino_ui_editor::Editor) -> Vec<NoteInstance> {
    let previews = editor.mask_previews();
    if previews.is_empty() && editor.mask_outline_notes().is_empty() {
        return Vec::new();
    }
    const GHOST_COLOR: [f32; 4] = [0.10, 0.10, 0.13, 1.0];
    const OUTLINE_COLOR: [f32; 4] = [0.55, 0.55, 0.62, 1.0];
    let mut out = Vec::new();

    // 1. ghost：暗色压住原音符
    for preview in &previews {
        for note in &preview.ghosts {
            out.push(NoteInstance::new_preview(
                note.tick,
                note.key.min(255) as u8,
                note.length,
                GHOST_COLOR,
            ));
        }
    }

    // 2. silent mask 轮廓：NoteInstance 没有虚线 primitive，用短段 + 间隔近似
    const DASH_TICKS: f32 = 8.0;
    const GAP_TICKS: f32 = 4.0;
    for note in editor.mask_outline_notes() {
        let end = note.tick + note.length.max(0.0);
        let mut cursor = note.tick;
        while cursor < end {
            out.push(NoteInstance::new_preview(
                cursor,
                note.key.min(255) as u8,
                DASH_TICKS.min(end - cursor),
                OUTLINE_COLOR,
            ));
            cursor += DASH_TICKS + GAP_TICKS;
        }
    }

    // 3. visible：可见片段用轨道色叠回
    for preview in &previews {
        for note in &preview.visible {
            out.push(NoteInstance::new_preview(
                note.tick,
                note.key.min(255) as u8,
                note.length,
                lumino_extras::palette::current_track_color_f32(note.track),
            ));
        }
    }
    out
}
