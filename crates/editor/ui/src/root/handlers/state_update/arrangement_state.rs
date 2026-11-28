//! 工程走带视图的状态更新：播放头、选区与音符编辑。
//!
//! 从 `state_update.rs` 的穷尽分派表按业务对象拆出；语句顺序、提前返回与
//! 状态读写顺序逐行保持原样。

use crate::root::Root;

/// 走带选择矩形：`(tick_start, tick_end, track_lo, track_hi)`
type ArrangeRect = (f64, f64, usize, usize);

impl Root {
    /// 设置演奏指示线位置并同步播放器
    pub(super) fn arrange_cursor(&mut self, tick: f64) -> bool {
        let tick_f = tick as f32;
        self.editor.playback_position = tick_f;
        if let Some(manager) = &mut self.playback.manager {
            manager.seek(tick_f);
        }
        true
    }

    /// 替换选区矩形（仅保留音轨范围内的有效区间）
    pub(super) fn arrange_selection(&mut self, rect: &Option<ArrangeRect>) -> bool {
        let data = &mut self.editor.editor_state.data;
        data.arrange_selection.clear();
        if let Some((tick_start, tick_end, track_lo, track_hi)) = rect {
            let ts = tick_start.max(0.0) as u32;
            let te = tick_end.max(0.0) as u32;
            if te > ts {
                let track_lo_u16 = (*track_lo).min(u16::MAX as usize) as u16;
                let track_hi_u16 = (*track_hi).min(u16::MAX as usize) as u16;
                data.arrange_selection
                    .add_rect_track(ts, te, 0, 127, track_lo_u16, track_hi_u16);
            }
        }
        true
    }

    /// 清空走带选区
    pub(super) fn arrange_selection_clear(&mut self) -> bool {
        self.editor.editor_state.data.arrange_selection.clear();
        true
    }

    /// 移动选中音符
    pub(super) fn arrange_move(&mut self, delta_ticks: i64, delta_tracks: i32) -> bool {
        // 框选误伤修复：arrange_move_notes 内部把选择集冻结为本次实际
        // 移动的音符（精确集合）；此处不得再平移选择矩形——矩形平移到
        // 落点会覆盖落点区域内既有的其他音符，导致后续操作误伤它们。
        let moved = self.editor.arrange_move_notes(delta_ticks, delta_tracks);
        if moved > 0 {
            self.update_playback_notes();
            self.editor.clear_notes_changed();
        }
        true
    }

    /// 擦除矩形范围内的音符
    pub(super) fn arrange_erase(
        &mut self,
        tick_start: f64,
        tick_end: f64,
        track_lo: usize,
        track_hi: usize,
    ) -> bool {
        let deleted = self
            .editor
            .arrange_erase(tick_start, tick_end, track_lo, track_hi);
        if deleted > 0 {
            self.update_playback_notes();
            self.editor.clear_notes_changed();
        }
        true
    }

    /// 在指定位置分割音符
    pub(super) fn arrange_razor(&mut self, tick: f64, track: usize) -> bool {
        let split = self.editor.arrange_razor(tick, track);
        if split > 0 {
            self.update_playback_notes();
            self.editor.clear_notes_changed();
        }
        true
    }

    /// 在指定音轨 tick 处添加音符
    pub(super) fn arrange_add_note(
        &mut self,
        track: usize,
        tick: f64,
        duration: f64,
        key: u8,
        velocity: u8,
    ) -> bool {
        let track_count = self.sidebar.tracks.len();
        let added = self
            .editor
            .arrange_add_note(track_count, track, tick, duration, key, velocity);
        if added {
            self.update_playback_notes();
            self.editor.clear_notes_changed();
        }
        true
    }

    /// 复制选中音符到剪贴板
    pub(super) fn arrange_copy(&mut self) -> bool {
        self.editor.arrange_copy_selected_notes();
        true
    }

    /// 从剪贴板粘贴音符
    pub(super) fn arrange_paste(&mut self) -> bool {
        let pasted = self.editor.arrange_paste_notes_from_clipboard();
        if pasted {
            self.update_playback_notes();
            self.editor.clear_notes_changed();
        }
        true
    }

    /// 剪切选中音符（复制 + 删除）
    pub(super) fn arrange_cut(&mut self) -> bool {
        let cut = self.editor.arrange_cut_selected_notes();
        if cut > 0 {
            self.editor.editor_state.data.arrange_selection.clear();
            self.update_playback_notes();
            self.editor.clear_notes_changed();
        }
        true
    }

    /// 删除选中音符
    pub(super) fn arrange_delete(&mut self) -> bool {
        let deleted = self.editor.arrange_delete_selected_notes();
        if deleted > 0 {
            self.editor.editor_state.data.arrange_selection.clear();
            self.update_playback_notes();
            self.editor.clear_notes_changed();
        }
        true
    }

    /// 全选（全部音轨 × 全部 tick）
    pub(super) fn arrange_select_all(&mut self) -> bool {
        // 全选只改选区、不改音符：无需 update_playback_notes，
        // 返回 true 触发 UI 重建（选框由 GPU 每帧按 arrange_selection 绘制）
        self.editor.arrange_select_all_notes();
        true
    }

    /// ghost 音符预览列表更新（元素为 tick 区间、视觉轨与 key）
    pub(super) fn arrange_ghost_notes(&mut self, notes: &[(f64, f64, usize, u8)]) -> bool {
        self.arrangement_view.ghost_notes = notes.to_vec();
        true
    }

    /// 拖拽中的框选矩形（实时预览，由 GPU 渲染）
    pub(super) fn arrange_drag_rect(&mut self, rect: &Option<ArrangeRect>) -> bool {
        self.arrangement_view.drag_sel_rect = *rect;
        true
    }
}
