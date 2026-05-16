//! 洋葱皮数据快照收集 —— 在主线程快速快照编辑状态，发送给 NoteWorker

use crate::host::Host;

impl Host {
    /// 收集洋葱皮计算所需的数据快照（非阻塞，O(1) 级别开销）
    pub(super) fn collect_onion_skin_snapshot(
        &self,
    ) -> super::note_worker::OnionSkinComputationSnapshot {
        let editor = &self.root.editor;
        let es = &editor.editor_state;
        let canvas_width = es.canvas.size.x;
        let keyboard_width = es.view.keyboard_width;
        let visible_tick_start = (es.view.scroll_x / es.view.zoom_x).max(0.0);
        let visible_tick_end = ((es.view.scroll_x + canvas_width - keyboard_width)
            / es.view.zoom_x)
            .max(visible_tick_start);
        let max_key = es.view.visible_key_count.saturating_sub(1);

        super::note_worker::OnionSkinComputationSnapshot {
            // 视口参数
            visible_tick_start,
            visible_tick_end,
            visible_key_min: 0u16,
            visible_key_max: max_key,
            // 洋葱皮数据
            onion_skin_enabled: editor.is_onion_skin_enabled(),
            track_onion_states: self.root.sidebar.get_onion_skin_states(),
            current_track: es.data.current_track,
            onion_skin_config: editor.onion_skin_config().clone(),
            document: es.data.document.clone(),
        }
    }
}
