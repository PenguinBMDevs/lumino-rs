//! 状态栏视图入口。
//!
//! 状态栏内容（左侧描述 / GPU 指标组 / CPU·MEM·FPS / 右侧文本 / 点 MEM 打开
//! 内存监控）已全部由 egui 绘制（`chrome::statusbar::show_model`）；iced 侧
//! 只剩 `statusbar.view()` 的同高同底色占位，用来保持钢琴卷帘区高度。

use crate::Element;
use crate::root::Root;

impl Root {
    /// 渲染状态栏（放置在 Stack 最底层）。
    pub(super) fn view_status_section(&self) -> Element<'_> {
        self.statusbar.view()
    }

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
