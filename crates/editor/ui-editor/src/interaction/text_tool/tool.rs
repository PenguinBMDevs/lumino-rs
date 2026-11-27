use super::{
    GlyphGrid, RasterScale, rasterize::point_in_rect, rasterize_text_for_tool, sample_to_notes_grid,
};
use crate::grid::text_tool_box::button_rects;
use crate::{EditState, Editor, Note};
use lumino_core::shape::{ShapeGeometry, ShapeKind, ShapeModel, ShapeOp, ShapeRect, TextOpV2};
use lumino_editor_state::text_tool::TextToolMode;
use lumino_editor_state::{ShapeNote, note_to_event};
use lumino_note_core::history::CreateOp;
use lumino_ui_core::geom::Point;

impl Editor {
    /// 文字工具：设置输入框文字（来自画布覆盖层 TextInput 的 on_input）
    pub(crate) fn set_text_tool_text(&mut self, text: String) {
        if self.editor_state.text_tool.active {
            self.editor_state.text_tool.text = text;
        }
    }

    /// 文字工具：写入面板参数并**自动切到 Parametric**
    ///
    /// 自动切换是有意保留的：面板 Mode 行即时显示当前路径，用户看得见，不算「静默」。
    /// 单位在 Font ⇄ Rows 之间切换时按同字体 cap height 比例换算字号，保持观感。
    pub(crate) fn set_text_params(&mut self, params: lumino_core::TextParams) {
        if params.validate().is_err() {
            return;
        }
        let prev = self.editor_state.text_tool.params.clone();
        let mut params = params;
        if params.unit != prev.unit {
            // 单位换算：`rows = font × cap_ratio`，反向除；clamp 4..96
            if let Some(ratio) = super::parametric::cap_ratio_for(&params) {
                params.size =
                    lumino_core::shape::convert_size_unit(prev.size, prev.unit, params.unit, ratio);
            }
        }
        self.editor_state.text_tool.params = params;
        self.editor_state.text_tool.layout_mode = lumino_core::TextLayoutMode::Parametric;
    }

    /// 文字工具：显式切换排版路径 LegacyBox / Parametric
    pub(crate) fn set_text_layout_mode(&mut self, mode: lumino_core::TextLayoutMode) {
        self.editor_state.text_tool.layout_mode = mode;
    }

    /// 参数面板 Reset：恢复默认排版参数，保留当前字体与排版模式。
    ///
    /// 字体是面板 Font 行的显式选择，重置参数不该顺带换字体；排版模式是独立控件，
    /// 重置后仍保持 LegacyBox / Parametric 不变。不走 `set_text_params`，
    /// 因此也不会触发「自动切 Parametric」。
    pub(crate) fn reset_text_params(&mut self) {
        let font = self.editor_state.text_tool.params.font.clone();
        self.editor_state.text_tool.params = lumino_core::TextParams {
            font,
            ..lumino_core::TextParams::default()
        };
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
        self.text_preview_cache.borrow_mut().take();
    }

    /// 当前音轨是否允许使用文字工具
    ///
    /// Conductor 音轨（track 0）不可放置音符，整工具在 Conductor 轨上不可用
    /// （与铅笔等工具的 `current_track == 0` 守卫同源）。
    pub fn text_tool_allowed(&self) -> bool {
        self.editor_state.data.current_track != 0
    }

    /// 文字工具：按下处理
    pub(crate) fn handle_text_tool_pressed(&mut self, pos: Point, key: u16) {
        // 文字工具的框是**生成框**（边界会成为生成音符的位置），X 向用 round 吸附
        // 到音符精度（见下方），与指针框选的「单元覆盖式」口径**刻意不同**。
        // 此处清空框选锚点，避免指针框选残留的锚点串味到文字工具链路。
        self.selection_ui.marquee_anchor = None;
        // Conductor 音轨（track 0）：整工具不可用，所有按下交互直接忽略
        if !self.text_tool_allowed() {
            return;
        }
        // 尚未拉框：落点若命中已 Bake 的文字形状，直接消费事件（只读，见下）
        if !self.editor_state.text_tool.active {
            let tick = self.pos_to_tick(pos);
            let key = self.pos_to_key(pos);
            if self.text_shape_at(tick, key).is_some() {
                // DEC-001：已 Bake 的文字形状只读，不再进入「重新编辑」；
                // 单击只消费事件，避免在旧文字上叠一个新的空文本框。
                tracing::debug!("文字形状已 Bake，只读");
                return;
            }
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
            // 框内点击（顶部输入条由 上一代 TextInput 覆盖，不会落到这里）：
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
        // 纵向卷帘：Y 是时间轴，框两端的 stored Y 取 tick 的屏幕 Y
        // （与 `pressed.rs` 纵向框选分支同口径）——横向才是"key 行上/下边"语义。
        let (start_y, current_y) = if self.editor_state.is_vertical_roll {
            let y = self.tick_to_y_vertical(tick);
            (y, y)
        } else {
            let view = &self.editor_state.view;
            let top_y = view.key_to_y(key);
            (top_y, top_y + view.zoom_y)
        };
        self.editor_state.interaction.edit_state = EditState::Selecting {
            start_tick: tick,
            start_key: key,
            current_tick: tick,
            current_key: key,
            start_y,
            current_y,
        };
    }

    /// 文字工具：移动处理（拖拽中实时更新框的 current）
    ///
    /// X 向实时吸附到音符精度，使框的横向长度变化按精度步进（与最终生成一致）。
    pub(crate) fn handle_text_tool_moved(&mut self, pos: Point) {
        let snap = self.editor_state.view.snap_precision.max(1.0);
        let tick = (self.pos_to_tick(pos) / snap).round() * snap;
        let key = self.pos_to_key(pos);
        // 纵向：stored Y = tick 的屏幕 Y（views 转置后 Y 是时间轴，key 行语义不适用）
        let new_y = if self.editor_state.is_vertical_roll {
            self.tick_to_y_vertical(tick)
        } else {
            let view = &self.editor_state.view;
            view.key_to_y(key) + view.zoom_y
        };
        if let EditState::Selecting {
            current_tick,
            current_key,
            current_y,
            ..
        } = &mut self.editor_state.interaction.edit_state
        {
            *current_tick = tick;
            *current_key = key;
            *current_y = new_y;
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

    /// 确认时按字形占位采样生成音符（√ 按钮）
    ///
    /// 按字形占位采样（轴角色见 [`GlyphGrid`]，横向/纵向各按视图轴向挂载，保证文字正着读）：
    /// 正常模式：每个有墨水的采样格生成一个音符，长度 = 音符精度；
    /// key 范围合并模式：**沿时间轴**把连续墨水合并为一个音符，任意空隙断开
    /// （不合并本应分开的笔画）。
    ///
    /// 预览与确认共用 [`Self::text_tool_grid_scale`]：同一文本（只按 `trim()` 判空，
    /// 不改写内容）、同一字体、同一采样比例——「看到的就是生成的」。
    ///
    /// 成功后清空文本框与编辑历史，写入当前轨并进入撤销栈。返回是否生成了音符。
    pub(crate) fn confirm_text_tool(&mut self) -> bool {
        let tt = self.editor_state.text_tool.clone();
        if !tt.active || tt.text.trim().is_empty() {
            return false;
        }
        // Conductor 音轨（track 0）禁止放置音符：与铅笔等工具（finish_drawing）一致，
        // 避免文字工具在不可编辑轨上创建音符。
        if self.editor_state.data.current_track == 0 {
            tracing::debug!("文字工具: Conductor 轨道禁止放置音符");
            return false;
        }
        let snap = self.editor_state.view.snap_precision;
        // 轴角色 + 采样比例的唯一决定点：预览（`grid/text_tool_box`）与生成必须同一份
        let Some((grid, scale)) = self.text_tool_grid_scale() else {
            return false;
        };
        let occupancy = match rasterize_text_for_tool(&tt, &tt.text, &grid, &scale) {
            Some(o) => o,
            None => return false,
        };

        let notes = sample_to_notes_grid(&occupancy, &grid, snap, tt.mode.is_merged());

        if notes.is_empty() {
            return false;
        }

        let track = self.editor_state.data.current_track;
        let model = build_text_shape_model(self, &tt, track, snap);

        let mut create_ops = Vec::with_capacity(notes.len());
        for &(tick, key, len) in &notes {
            let note = Note::new(tick, key, len);
            // 带稳定 ID 逐条插入：redo 复用同一批 ID，撤销/重做保持幂等
            if self
                .editor_state
                .data
                .insert_note_with_id(track, note.clone())
                .is_some()
            {
                create_ops.push(CreateOp {
                    track_id: track as u32,
                    note: note_to_event(note),
                });
            }
        }
        if create_ops.is_empty() {
            return false;
        }

        let group = self
            .editor_state
            .data
            .history
            .push_note_create_new_group(create_ops);
        self.editor_state.data.mark_current_track_changed();
        let shape_id = self.editor_state.shape_store.insert(model, group);
        self.editor_state.shape_store.assign_notes(
            shape_id,
            notes
                .iter()
                .map(|&(tick, key, length)| ShapeNote {
                    track,
                    tick,
                    key,
                    length,
                    velocity: 100,
                    channel: 0,
                })
                .collect(),
        );

        self.editor_state.text_tool.reset();
        self.text_preview_cache.borrow_mut().take();
        self.mark_notes_changed();
        true
    }

    /// 文字工具的采样网格 + 栅格比例（预览 / 确认 / 重烘焙的统一入口）
    ///
    /// 比例权威是**实时视图**的 `k = zoom_y / (ppq × zoom_x)`；制字时把它冻结进
    /// `TextOpV2.k`，重烘焙就不再随当前缩放漂移。
    pub(crate) fn text_tool_grid_scale(&self) -> Option<(GlyphGrid, RasterScale)> {
        let tt = &self.editor_state.text_tool;
        let grid = GlyphGrid::from_state(
            tt,
            self.editor_state.view.snap_precision,
            self.editor_state.is_vertical_roll,
        );
        if grid.cols == 0 || grid.rows == 0 {
            return None;
        }
        let scale = RasterScale::from_k(
            grid.vertical,
            grid.tick_step,
            f32::from(self.editor_state.view.ppq),
            self.live_arc_k() as f32,
            grid.cols,
            grid.rows,
        );
        Some((grid, scale))
    }

    /// 栅格签名：任何会改变采样结果的几何 / 比例输入
    ///
    /// 预览纹理缓存按它失效——漏掉 snap / zoom / k 会让缩放后复用旧纹理。
    pub fn text_raster_key(&self) -> Option<String> {
        let (grid, scale) = self.text_tool_grid_scale()?;
        Some(format!(
            "{}x{}:snap{:.3}:v{}:k{:.6}:ppq{:.1}:ss{:.4}x{:.4}:ppk{:.4}",
            grid.cols,
            grid.rows,
            grid.tick_step,
            grid.vertical,
            self.live_arc_k(),
            self.editor_state.view.ppq,
            scale.ss_x,
            scale.ss_y,
            scale.px_per_key,
        ))
    }
}

/// 由文本框几何与参数构造新的文字形状模型（Bake 时登记）
fn build_text_shape_model(
    editor: &Editor,
    tt: &lumino_editor_state::text_tool::TextToolState,
    track: usize,
    snap: f32,
) -> ShapeModel {
    // 视图比例随形状冻结：重烘焙必须复现制字时的屏幕格比例
    let k = editor.live_arc_k() as f32;
    let mut model = ShapeModel::new(0, track as u16, ShapeKind::Custom, k);
    model.track = track as u16;
    model.kind = ShapeKind::Custom;
    let (tick_lo, tick_hi) = tt.normalized_ticks();
    let (key_lo, key_hi) = tt.normalized_keys();
    model.geometry = ShapeGeometry::from_rect(ShapeRect::normalized(
        tick_lo,
        key_lo as f32,
        tick_hi,
        key_hi as f32,
    ));
    model
        .ops
        .retain(|op| !matches!(op, ShapeOp::Text(_) | ShapeOp::TextV2(_)));
    model.ops.push(ShapeOp::TextV2(TextOpV2 {
        text: tt.text.clone(),
        params: tt.params.clone(),
        layout: tt.layout_mode,
        snap,
        merged: tt.mode.is_merged(),
        vertical: editor.editor_state.is_vertical_roll,
        k,
    }));
    model.name = tt.text.chars().take(32).collect();
    model
}
