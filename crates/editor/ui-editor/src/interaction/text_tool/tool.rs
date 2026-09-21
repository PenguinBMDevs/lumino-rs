use super::{rasterize::point_in_rect, rasterize_text, sample_to_notes};
use crate::grid::text_tool_box::button_rects;
use crate::{EditState, Editor, Note};
use iced_core::Point;
use lumino_editor_state::note_to_event;
use lumino_editor_state::text_tool::TextToolMode;
use lumino_note_core::history::CreateOp;

impl Editor {
    /// 文字工具：设置输入框文字（来自画布覆盖层 TextInput 的 on_input）
    pub(crate) fn set_text_tool_text(&mut self, text: String) {
        if self.editor_state.text_tool.active {
            self.editor_state.text_tool.text = text;
        }
    }

    /// 文字工具：切换采样模式（正常 / key 范围合并）
    pub(crate) fn toggle_text_tool_mode(&mut self) {
        let m = &mut self.editor_state.text_tool.mode;
        *m = if m.is_merged() {
            TextToolMode::Normal
        } else {
            TextToolMode::KeyRangeMerged
        };
    }

    /// 文字工具：取消并清空文本框
    pub(crate) fn cancel_text_tool(&mut self) {
        self.editor_state.text_tool.reset();
    }

    /// 当前音轨是否允许使用文字工具
    ///
    /// Conductor 音轨（track 0）不可放置音符，整工具在 Conductor 轨上不可用
    /// （与铅笔等工具的 `current_track == 0` 守卫同源）。
    pub(crate) fn text_tool_allowed(&self) -> bool {
        self.editor_state.data.current_track != 0
    }

    /// 文字工具：按下处理
    pub(crate) fn handle_text_tool_pressed(&mut self, pos: Point, key: u16) {
        // Conductor 音轨（track 0）：整工具不可用，所有按下交互直接忽略
        if !self.text_tool_allowed() {
            return;
        }
        // 已拉出框：先判按钮命中
        if self.editor_state.text_tool.active {
            if let Some(btns) = button_rects(self) {
                if point_in_rect(pos, btns.confirm) {
                    self.confirm_text_tool();
                    return;
                }
                if point_in_rect(pos, btns.cancel) {
                    self.cancel_text_tool();
                    return;
                }
                if point_in_rect(pos, btns.mode) {
                    self.toggle_text_tool_mode();
                    return;
                }
            }
            // 框内点击（顶部输入条由 iced TextInput 覆盖，不会落到这里）：
            // 拖拽中间实心区域可整体移动文本框。
            if let Some((l, t, r, b)) = crate::grid::text_tool_box::box_rect_screen(self)
                && pos.x >= l
                && pos.x <= r
                && pos.y >= t
                && pos.y <= b
            {
                let grab_tick = self.pos_to_tick(pos);
                let grab_key = self.pos_to_key(pos) as f32;
                self.editor_state.text_tool.begin_move(grab_tick, grab_key);
                self.editor_state.text_tool.editing = true;
                return;
            }
            // 框外点击：取消当前框，开始拉新框
            self.cancel_text_tool();
        }

        // 新框：进入 Selecting 拖拽（Y 向吸附 key 行；X 向吸附音符精度，
        // 使拖框过程的拉伸变化按精度步进，与最终生成的音符列对齐）。
        let snap = self.editor_state.view.snap_precision.max(1.0);
        let tick = (self.pos_to_tick(pos) / snap).round() * snap;
        let view = &self.editor_state.view;
        let top_y = view.key_to_y(key);
        let bottom_y = top_y + view.zoom_y;
        self.editor_state.interaction.edit_state = EditState::Selecting {
            start_tick: tick,
            start_key: key,
            current_tick: tick,
            current_key: key,
            start_y: top_y,
            current_y: bottom_y,
        };
    }

    /// 文字工具：移动处理（拖拽中实时更新框的 current）
    ///
    /// X 向实时吸附到音符精度，使框的横向长度变化按精度步进（与最终生成一致）。
    pub(crate) fn handle_text_tool_moved(&mut self, pos: Point) {
        let snap = self.editor_state.view.snap_precision.max(1.0);
        let tick = (self.pos_to_tick(pos) / snap).round() * snap;
        let key = self.pos_to_key(pos);
        if let EditState::Selecting {
            current_tick,
            current_key,
            current_y,
            ..
        } = &mut self.editor_state.interaction.edit_state
        {
            *current_tick = tick;
            *current_key = key;
            let view = &self.editor_state.view;
            *current_y = view.key_to_y(key) + view.zoom_y;
        }
    }

    /// 文字工具：拖拽移动已放置的文本框（中间实心区域）
    ///
    /// 保持框尺寸，整体平移；X 向按音符精度、Y 向按 key 行吸附（与采样/生成一致）。
    pub(crate) fn handle_text_tool_box_move(&mut self, pos: Point) {
        let snap = self.editor_state.view.snap_precision;
        let cur_tick = self.pos_to_tick(pos);
        let cur_key = self.pos_to_key(pos) as f32;
        self.editor_state.text_tool.move_to(cur_tick, cur_key, snap);
    }

    /// 文字工具：确认生成音符（√ 按钮）
    ///
    /// 按字形占位采样：
    /// 正常模式：每个有墨水的 (col,row) 生成一个音符，长度 = 音符精度；
    /// key 范围合并模式：每个 key 行内连续有墨水的列合并为一个音符，任意空隙断开（不合并本应分开的笔画）。
    ///
    /// 成功后清空文本框与编辑历史，写入当前轨并进入撤销栈。返回是否生成了音符。
    pub(crate) fn confirm_text_tool(&mut self) -> bool {
        let tt = self.editor_state.text_tool.clone();
        if !tt.has_content() {
            return false;
        }
        // Conductor 音轨（track 0）禁止放置音符：与铅笔等工具（finish_drawing）一致，
        // 避免文字工具在不可编辑轨上创建音符。
        if self.editor_state.data.current_track == 0 {
            tracing::debug!("文字工具: Conductor 轨道禁止放置音符");
            return false;
        }
        let snap = self.editor_state.view.snap_precision;
        let (tick_lo, _) = tt.normalized_ticks();
        let (_key_lo, key_hi) = tt.normalized_keys();
        let cols = tt.cols(snap);
        let rows = tt.rows();
        if cols == 0 || rows == 0 {
            return false;
        }
        let occupancy = match rasterize_text(&tt.text, cols, rows, tt.font_family) {
            Some(o) => o,
            None => return false,
        };

        // 行 0 = 文字顶部 = 最高 key
        let key_top = key_hi as i32;
        let notes = sample_to_notes(&occupancy, tick_lo, key_top, snap, tt.mode.is_merged());

        if notes.is_empty() {
            return false;
        }

        let track = self.editor_state.data.current_track;
        let mut create_ops = Vec::with_capacity(notes.len());
        for (tick, key, len) in notes {
            let note = Note::new(tick, key, len);
            if self.editor_state.data.insert_note(track, note.clone()) {
                create_ops.push(CreateOp {
                    track_id: track as u32,
                    note: note_to_event(note),
                });
            }
        }
        if create_ops.is_empty() {
            return false;
        }

        self.editor_state.data.history.push_note_create(create_ops);
        self.editor_state.data.mark_current_track_changed();
        self.editor_state.text_tool.reset();
        self.mark_notes_changed();
        true
    }
}
