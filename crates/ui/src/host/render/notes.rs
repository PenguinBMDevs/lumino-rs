//! 音符数据收集 —— 在主线程快速快照编辑状态，发送给 NoteWorker

use crate::host::Host;

impl Host {
    /// 收集音符计算所需的所有数据快照（非阻塞，O(1) 级别开销）
    ///
    /// 洋葱皮实例的计算在 NoteWorker 线程中完成，这里只收集输入数据。
    /// - `im::Vector<Note>::clone()` 是 O(1) 结构共享
    /// - `Arc::clone()` 是 O(1) refcount bump
    /// - `EditState::clone()` 只有少量标量
    /// - `ViewState::clone()` / `CanvasState` 全是标量
    pub(super) fn collect_note_snapshot(&self) -> super::note_worker::NoteComputationSnapshot {
        let editor = &self.root.editor;
        let es = &editor.editor_state;
        let canvas_width = es.canvas.size.x;
        let keyboard_width = es.view.keyboard_width;
        let visible_tick_start = (es.view.scroll_x / es.view.zoom_x).max(0.0);
        let visible_tick_end = ((es.view.scroll_x + canvas_width - keyboard_width)
            / es.view.zoom_x)
            .max(visible_tick_start);
        let max_key = es.view.visible_key_count.saturating_sub(1);

        super::note_worker::NoteComputationSnapshot {
            // 主音轨音符（im::Vector clone O(1)）
            notes: es.data.notes.clone(),
            // 视口参数
            visible_tick_start,
            visible_tick_end,
            visible_key_min: 0u16,
            visible_key_max: max_key,
            // 编辑参数
            default_note_length: es.view.default_note_length,
            snap_precision: es.view.snap_precision,
            // 编辑状态
            edit_state: es.interaction.edit_state.clone(),
            // 洋葱皮数据
            onion_skin_enabled: editor.is_onion_skin_enabled(),
            track_onion_states: self.root.sidebar.get_onion_skin_states(),
            current_track: es.data.current_track,
            onion_skin_config: editor.onion_skin_config().clone(),
            document: es.data.document.clone(),
        }
    }
}
