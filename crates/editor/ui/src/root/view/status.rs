//! 状态栏视图渲染函数
//!
//! 状态栏的渲染委托给 statusbar.view()。

use crate::Element;
use crate::root::Root;
use lumino_core::storage::config::NoteCountDisplay;

impl Root {
    /// 渲染状态栏（性能面板已交由 Stack 浮动层处理）
    pub(super) fn view_status_section(&self) -> Element<'_> {
        // UI-015：仅"下边栏"模式把音符总量交给状态栏（顶替空闲时的「就绪」）；
        // "工程设置面板"模式下状态栏保持现状。
        let note_count = match self.settings.display.note_count_display {
            NoteCountDisplay::StatusBar => Some(self.editor_note_count()),
            NoteCountDisplay::ProjectSettings => None,
        };
        self.statusbar
            .view(self.settings.display.language, note_count)
    }

    /// 当前工程音符总量（O(轨道数)；UI-015 状态栏显示用）
    fn editor_note_count(&self) -> usize {
        let Some(doc) = self.editor.editor_state.data.document.as_ref() else {
            return 0;
        };
        (0..doc.track_count())
            .map(|track_idx| doc.track_notes(track_idx).len())
            .sum()
    }
}
