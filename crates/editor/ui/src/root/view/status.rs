//! 状态栏用的工程统计。
//!
//! 状态栏内容（左侧描述 / GPU 指标组 / CPU·MEM·FPS / 右侧文本 / 点 MEM 打开
//! 内存监控）已全部由 egui 绘制（`chrome::statusbar::show_model`），iced 视图已删除。

use crate::root::Root;

impl Root {
    /// 当前工程的音符总数（O(轨道数)，UI-015 状态栏显示用）。
    pub(super) fn editor_note_count(&self) -> usize {
        let Some(doc) = self.editor.editor_state.data.document.as_ref() else {
            return 0;
        };
        (0..doc.track_count())
            .map(|track_idx| doc.track_notes(track_idx).len())
            .sum()
    }
}
