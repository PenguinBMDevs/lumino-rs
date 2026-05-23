//! 洋葱皮数据快照收集 —— 在主线程快速快照编辑状态，发送给 NoteWorker

use crate::host::Host;

impl Host {
    /// 收集洋葱皮计算所需的数据快照（非阻塞，O(1) 级别开销）
    pub(super) fn collect_onion_skin_snapshot(
        &self,
        viewport_logical_size: (f32, f32),
    ) -> super::note_worker::OnionSkinComputationSnapshot {
        let editor = &self.root.editor;
        let es = &editor.editor_state;
        let canvas = &es.canvas;
        let view = &es.view;
        let canvas_width = canvas.size.x;
        let canvas_height = canvas.size.y;
        let keyboard_width = view.keyboard_width;
        let canvas_offset_x = canvas.offset.x;
        let canvas_offset_y = canvas.offset.y;

        let visible_tick_start = (view.scroll_x / view.zoom_x).max(0.0);
        let visible_tick_end =
            ((view.scroll_x + canvas_width - keyboard_width) / view.zoom_x).max(visible_tick_start);

        // 用 scroll_y 计算真实的可见 key 范围（与 rendering.rs 保持一致）
        let max_key_index = (view.visible_key_count - 1) as f32;
        let viewport_height = (canvas_height - view.ruler_height).max(0.0);
        let key_top_f32 = max_key_index - (view.scroll_y / view.zoom_y);
        let key_bottom_f32 = max_key_index - ((view.scroll_y + viewport_height) / view.zoom_y);
        let visible_key_max = key_top_f32.ceil() as u16 + 1;
        let visible_key_min = (key_bottom_f32.floor().max(0.0) as u16).saturating_sub(1);

        super::note_worker::OnionSkinComputationSnapshot {
            // 视口参数（用于瓦片过滤）
            visible_tick_start,
            visible_tick_end,
            visible_key_min,
            visible_key_max,
            // 视口参数（用于 NDC 坐标计算，与 note.wgsl 一致）
            scroll_x: view.scroll_x,
            scroll_y: view.scroll_y,
            zoom_x: view.zoom_x,
            zoom_y: view.zoom_y,
            keyboard_width,
            ruler_height: view.ruler_height,
            canvas_offset_x,
            canvas_offset_y,
            viewport_logical_width: viewport_logical_size.0,
            viewport_logical_height: viewport_logical_size.1,
            max_key_index,
            // 洋葱皮数据
            onion_skin_enabled: editor.is_onion_skin_enabled(),
            track_onion_states: self.root.sidebar.get_onion_skin_states(),
            current_track: es.data.current_track,
            document: es.data.document.clone(),
        }
    }
}
