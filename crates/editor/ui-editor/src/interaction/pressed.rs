//! 鼠标按下事件处理 — 工具分发、音符编辑/绘制
//!
//! 包含：按下事件 → 工具分发 → 指针/铅笔/橡皮擦/默认工具处理
//!       音符编辑开始、绘制开始、音符音频播放、音符添加事件发射

use crate::{Editor, HitType, Note};
use lumino_core::storage::config::EraserBehavior;
use lumino_editor_state::DragState;
use lumino_editor_state::editor_state::interaction_ops;
use lumino_message::Tool;
use lumino_ui_core::constants::editor::{DEFAULT_MIDI_CHANNEL, DEFAULT_NOTE_VELOCITY};

impl Editor {
    /// 处理鼠标按下事件
    pub(crate) fn handle_pressed(&mut self, pos: iced_core::Point, shift: bool) {
        if !self.is_inside_canvas(pos) {
            return;
        }

        let tick = self.pos_to_tick(pos);
        let key = self.pos_to_key(pos);
        let snapped_tick = self.snap_tick(tick);

        self.handle_tool_pressed(pos, shift, snapped_tick, key);
    }

    /// 根据当前工具处理鼠标按下事件
    pub(crate) fn handle_tool_pressed(
        &mut self,
        pos: iced_core::Point,
        shift: bool,
        snapped_tick: f32,
        key: u16,
    ) {
        // 图片转 MIDI 放置模式：拦截全部按下交互
        if self.editor_state.image_to_midi.is_active() {
            self.handle_i2m_pressed(pos, snapped_tick, key as f32);
            return;
        }

        let hit_result = self.hit_test_note(pos);

        match self.editor_state.tool {
            Tool::Pointer => self.handle_pointer_pressed(pos, hit_result, snapped_tick),
            // Y 向框选工具：行为与 Pointer 相同，但在 Selecting 状态下 Y 维度自动覆盖全部可见键
            Tool::PointerYSelect => self.handle_pointer_pressed(pos, hit_result, snapped_tick),
            Tool::Pencil => self.handle_pencil_pressed(pos, hit_result, snapped_tick, key),
            Tool::Curve => {
                // Conductor 音轨（track 0）：整工具不可用，不开始任何曲线/填充绘制
                if self.editor_state.data.current_track == 0 {
                    return;
                }
                if self.editor_state.line_tool.fill_enabled {
                    // 颜料桶模式：点击封闭区域内部 → 泛洪填充生成实心音符
                    self.handle_fill_pressed(pos, snapped_tick, key);
                } else {
                    // 正常模式：两点拉出路径（直线/贝塞尔），
                    // 点击曲线段插入锚点弯曲，√ 批量生成音符。
                    self.handle_line_tool_pressed(pos, snapped_tick, key as f32);
                }
            }
            Tool::Eraser | Tool::DrawEraser => self.handle_eraser_pressed(pos, shift, hit_result),
            Tool::Brush => self.handle_brush_pressed(pos, hit_result, snapped_tick, key),
            Tool::Text => self.handle_text_tool_pressed(pos, key),
            _ => self.handle_default_tool_pressed(pos, hit_result, snapped_tick, key),
        }
    }

    /// 指针工具：框选或编辑现有音符
    ///
    /// **命中优先级**（关键交互逻辑）：
    /// 1. 若已有选中音符，优先检测选择框（`hit_test_selection_box`）：
    ///    - `Inside`：框选框内任意位置 → `DraggingSelection`（拖动全部选中音符）
    ///    - `LeftEdge/RightEdge`：框选框左右边缘 → `ResizingSelectionStart/End`（拉伸框选边缘）
    ///    - `None`：点击在框选框外 → 回退到音符命中检测
    /// 2. 若未命中选择框（或无选中音符），检测音符命中（`hit_test_note`）：
    ///    - 命中音符 → 单音符编辑（`ResizingStart/End/PendingDrag`）
    /// 3. 都未命中 → 点击空白处，提交 pending 拖动 + 开始新框选
    ///
    /// **修复历史**：原实现 `hit_test_note` 优先于 `hit_test_selection_box`，导致框选框内点击
    /// 若命中某个选中音符的边缘，会误进入单音符 `ResizingStart/End` 状态，框选拖动无法触发。
    /// 调整优先级后，框选框内任意位置都走框选逻辑，符合用户"按住框选框内任意位置移动即拖动"的预期。
    pub(crate) fn handle_pointer_pressed(
        &mut self,
        pos: iced_core::Point,
        hit_result: Option<(usize, HitType)>,
        snapped_tick: f32,
    ) {
        let tick = self.pos_to_tick(pos);
        let key = self.pos_to_key(pos);
        // Y 向框选工具：X 维度同普通框选，Y 维度自动覆盖全部可见键
        let is_y_select = self.editor_state.tool == Tool::PointerYSelect;
        // 左右边界 = 鼠标精确 tick 位置（像素级，不吸附）：
        // 起点若吸附到网格点，选区左边界会比鼠标按下位置多延伸最多一个精度单元，
        // 与移动时的精确 current_tick 不对称，必须保持两边一致的精确语义。
        let selection_start_tick = tick;

        // 优先级 1：有选中音符时，先检测选择框命中
        // 选择框命中时，无论是否同时命中音符，都走框选逻辑（避免边缘误判走单音符拉伸）
        let has_selection = self.has_selection();
        let sel_hit = if has_selection {
            self.hit_test_selection_box(pos)
        } else {
            None
        };

        if let Some(sel_hit_type) = sel_hit {
            // 命中选择框：根据边缘/内部分别进入调整大小或拖动状态
            match sel_hit_type {
                crate::SelectionHitType::Inside => {
                    // ghost 方案（累积模式）：从选中集合构建 DragState
                    let note_count = self.editor_state.data.current_track_note_count();
                    let drag_state = DragState::from_indices(
                        self.get_selected_indices(),
                        note_count,
                        snapped_tick as i64,
                        key as i16,
                    );
                    // 复制模式判定：
                    // - 无 pending_copy（首次复制）：唯一框 = 选中集合框，
                    //   Ctrl + 拖动 = 复制（原件不动、副本跟手）
                    // - 有 pending_copy（复制未提交）：唯一框 = **副本框**
                    //   （只保留最新件框选，原件不再框选）→ Ctrl + 拖动 =
                    //   从副本继续复制下一份
                    // - 无 Ctrl：移动（DraggingSelection）
                    // 原件区域（复制模式下无框）走 priority 2 单音符命中 → 移动单个原件
                    if self.ctrl_pressed() {
                        // Ctrl + 拖动（首次复制 / 从副本框继续复制）：
                        // 原始音符不动，副本跟随鼠标预览，松手后待点击空白处才写入内存层
                        self.editor_state.interaction.edit_state =
                            crate::EditState::DraggingSelectionCopy { drag_state };
                    } else {
                        // NoteMove 操作日志化：批量拖动期间不 push 快照，
                        // 松手时构造 MoveOp 异步提交。
                        self.editor_state.interaction.edit_state =
                            crate::EditState::DraggingSelection { drag_state };
                    }
                }
                crate::SelectionHitType::LeftEdge => {
                    // 框选左边缘拉伸：先提交 pending 拖动（保留选区，要在当前选区上拉伸）
                    // 注意：不能用 flush_pending_drag（会清空 selected_notes，导致拉伸无目标）
                    // 未提交的复制（pending_copy）与新操作冲突：直接丢弃（不写入），
                    // 否则 commit_pending_copy 会替换选中集合，导致拉伸作用到副本上。
                    self.pending_copy_drag_state = None;
                    if self.pending_drag_state.is_some() {
                        self.commit_pending_drag();
                        // commit_pending_drag 移动音符后，selected_bounds 缓存仍为原始位置，
                        // 不失效会导致后续 get_selection_box_bounds 返回错误边界，框选框跳变。
                        // 在下一次访问时通过 O(N) 回退或 ghost 路径重建缓存。
                        self.selected_bounds.set(None);
                    }
                    self.push_history();
                    self.editor_state.interaction.edit_state =
                        crate::EditState::ResizingSelectionStart {
                            origin_tick: snapped_tick,
                            last_tick: snapped_tick,
                        };
                }
                crate::SelectionHitType::RightEdge => {
                    // 框选右边缘拉伸：同 LeftEdge，提交 pending 但保留选区
                    // 未提交的复制（pending_copy）同样丢弃（见 LeftEdge 注释）
                    self.pending_copy_drag_state = None;
                    if self.pending_drag_state.is_some() {
                        self.commit_pending_drag();
                        // 同 LeftEdge：清除 selected_bounds 缓存，防止框选框跳变
                        self.selected_bounds.set(None);
                    }
                    self.push_history();
                    self.editor_state.interaction.edit_state =
                        crate::EditState::ResizingSelectionEnd {
                            origin_tick: snapped_tick,
                            last_tick: snapped_tick,
                        };
                }
            }
        } else if let Some((index, hit_type)) = hit_result {
            // 优先级 2：未命中选择框但命中音符 → 单音符编辑
            // （点击在框选框外，或无选中音符时点击音符）
            self.flush_pending_drag();
            if !self
                .editor_state
                .interaction
                .selected_notes
                .contains(&index)
            {
                self.selection_clear();
                self.selection_insert(index);
            }
            self.start_note_edit(index, hit_type, pos);
        } else {
            // 优先级 3：都未命中 → 点击空白处，提交 pending 拖动 + 开始新框选
            self.flush_pending_drag();
            self.playback_position = snapped_tick;
            self.selection_clear();
            // Y 向框选工具：Y 维度自动覆盖全部可见键范围（纵向转置：Y 为 tick，X 为 key）
            let (start_key, current_key, start_y, current_y) = if is_y_select {
                if self.editor_state.is_vertical_roll {
                    let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);
                    let grid_bottom =
                        self.editor_state.canvas.size_y - self.editor_state.view.keyboard_width;
                    // 纵向 Y=时间，覆盖全高；key 覆盖全键盘（X 维度）
                    (max_key, 0, 0.0, grid_bottom.max(0.0))
                } else {
                    let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);
                    let top_y = self.editor_state.view.key_to_y(max_key);
                    let bottom_y =
                        self.editor_state.view.key_to_y(0) + self.editor_state.view.zoom_y;
                    (max_key, 0, top_y, bottom_y)
                }
            } else if self.editor_state.is_vertical_roll {
                // 纵向：Y 为时间轴，起点为 tick 对应的屏幕 Y（零高初始框，拖动时扩展）
                let y = self.tick_to_y_vertical(tick);
                (key, key, y, y)
            } else {
                // 上下精度 = 单个 key：起点/终点对齐到 key 线
                let view = &self.editor_state.view;
                let top_y = view.key_to_y(key);
                let bottom_y = top_y + view.zoom_y;
                (key, key, top_y, bottom_y)
            };
            self.editor_state.interaction.edit_state = crate::EditState::Selecting {
                start_tick: selection_start_tick,
                start_key,
                current_tick: selection_start_tick,
                current_key,
                start_y,
                current_y,
            };
        }
    }

    /// 提交 pending 批量拖动/复制并清空选区（非累积场景调用）
    ///
    /// 在用户开始新操作（点击音符/调整大小/点击空白处）时调用。
    /// 累积拖动场景（框选内部命中）不调用此方法，保留 pending。
    ///
    /// **提交顺序（正确性关键）**：移动（pending_drag）走异步提交，完成时
    /// 会整轨替换音符——若复制（pending_copy）先插入副本，会被异步结果覆盖。
    /// 因此当两者并存时：先启动移动异步提交 → `drain_async_commit` 等待完成
    /// （document 回到一致状态）→ 再基于最新索引写入副本。
    fn flush_pending_drag(&mut self) {
        if self.pending_drag_state.is_some() {
            self.commit_pending_drag();
            // 移动 + 复制并存：必须等移动异步提交完成（整轨替换），
            // 否则 replace_track_notes 会覆盖刚插入的副本
            if self.pending_copy_drag_state.is_some() {
                self.drain_async_commit();
            }
            self.selection_clear();
        }
        if self.pending_copy_drag_state.is_some() {
            self.commit_pending_copy();
        }
    }

    /// 铅笔工具：放置新音符或编辑现有音符
    pub(crate) fn handle_pencil_pressed(
        &mut self,
        pos: iced_core::Point,
        hit_result: Option<(usize, HitType)>,
        snapped_tick: f32,
        key: u16,
    ) {
        if let Some((index, hit_type)) = hit_result {
            self.start_note_edit(index, hit_type, pos);
        } else {
            self.start_drawing(snapped_tick, key);
        }
    }

    /// 橡皮擦工具：删除音符
    pub(crate) fn handle_eraser_pressed(
        &mut self,
        pos: iced_core::Point,
        shift: bool,
        hit_result: Option<(usize, HitType)>,
    ) {
        let tick = self.pos_to_tick(pos);
        let key = self.pos_to_key(pos);
        // 左右边界 = 鼠标精确 tick 位置（像素级，不吸附，与指针工具框选一致）
        let selection_start_tick = tick;

        match self.editor_state.view.eraser_behavior {
            EraserBehavior::Default => {
                if shift {
                    self.selection_clear();
                    let (start_y, current_y) = if self.editor_state.is_vertical_roll {
                        let y = self.tick_to_y_vertical(tick);
                        (y, y)
                    } else {
                        let y0 = self.editor_state.view.key_to_y(key);
                        (y0, y0 + self.editor_state.view.zoom_y)
                    };
                    self.editor_state.interaction.edit_state = crate::EditState::Selecting {
                        start_tick: selection_start_tick,
                        start_key: key,
                        current_tick: selection_start_tick,
                        current_key: key,
                        start_y,
                        current_y,
                    };
                } else if hit_result.is_some() {
                    self.delete_note_at(pos);
                }
            }
            EraserBehavior::DirectSelect => {
                if shift && hit_result.is_some() {
                    self.delete_note_at(pos);
                } else {
                    self.selection_clear();
                    let (start_y, current_y) = if self.editor_state.is_vertical_roll {
                        let y = self.tick_to_y_vertical(tick);
                        (y, y)
                    } else {
                        let y0 = self.editor_state.view.key_to_y(key);
                        (y0, y0 + self.editor_state.view.zoom_y)
                    };
                    self.editor_state.interaction.edit_state = crate::EditState::Selecting {
                        start_tick: selection_start_tick,
                        start_key: key,
                        current_tick: selection_start_tick,
                        current_key: key,
                        start_y,
                        current_y,
                    };
                }
            }
        }
    }

    /// 其他工具：默认使用铅笔工具逻辑
    pub(crate) fn handle_default_tool_pressed(
        &mut self,
        pos: iced_core::Point,
        hit_result: Option<(usize, HitType)>,
        snapped_tick: f32,
        key: u16,
    ) {
        if let Some((index, hit_type)) = hit_result {
            self.start_note_edit(index, hit_type, pos);
        } else {
            self.start_drawing(snapped_tick, key);
        }
    }

    /// 开始编辑现有音符
    pub(crate) fn start_note_edit(
        &mut self,
        index: usize,
        hit_type: HitType,
        pos: iced_core::Point,
    ) {
        interaction_ops::start_note_edit(
            &mut self.editor_state.data,
            &mut self.editor_state.interaction,
            index,
            hit_type,
            (pos.x, pos.y),
        );
    }

    /// 开始绘制新音符
    fn start_drawing(&mut self, snapped_tick: f32, key: u16) {
        interaction_ops::start_drawing(&mut self.editor_state.interaction, snapped_tick, key);
    }

    /// 播放音符音频
    pub(crate) fn play_note_audio(&mut self, key: u16, _context: &str) {
        self.editor_state
            .interaction
            .play_note_audio(key, DEFAULT_NOTE_VELOCITY);
    }

    /// 发送新音符添加的协作同步事件
    pub(super) fn emit_note_added_event(&self, note: &Note) {
        lumino_message::events::emit(lumino_message::events::Event::Window(
            lumino_message::events::window::Event::local_note_added(
                note.id,
                note.tick,
                note.key,
                note.length,
                DEFAULT_NOTE_VELOCITY,
                DEFAULT_MIDI_CHANNEL,
                self.editor_state.data.current_track,
            ),
        ));
    }
}
