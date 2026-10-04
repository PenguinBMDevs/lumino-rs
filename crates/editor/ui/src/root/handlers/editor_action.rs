//! 编辑器动作与钢琴卷帘上下文菜单处理器
//!
//! 处理 `Message::EditorAction` 与 `Message::PianoRollContextMenu`，
//! 将编辑操作委托给 Editor，并负责播放引擎同步。

use crate::message::EditorAction;
use crate::root::Root;
use lumino_message::{
    ContextMenuTarget, PianoRollContextMenuAction, PianoRollContextMenuItem,
};

impl Root {
    /// 处理编辑器动作
    ///
    /// 返回 `true` 表示音符数据确实发生了变化。
    pub(crate) fn handle_editor_action(&mut self, action: EditorAction) -> bool {
        puffin::profile_function!();
        // 演奏指示线移动与滚动不修改音符数据，直接返回 false，
        // 避免被误判为脏音轨而触发昂贵的后台重生成。
        let is_playhead_or_scroll = matches!(
            action,
            EditorAction::Scrubbed { .. }
                | EditorAction::IndicatorDragStart { .. }
                | EditorAction::IndicatorDragMove { .. }
                | EditorAction::Scrolled { .. }
        );

        // 编辑拦截：Undo/Redo 在编辑状态下被 Editor::undo/redo 拦截，
        // 这里检测拦截并按 UiConfig 设置显示 Toast 提示用户。
        if matches!(action, EditorAction::Undo | EditorAction::Redo) && self.editor.is_editing() {
            if self.intercept_notification_enabled() {
                self.toast.push(
                    crate::toast::ToastLevel::Warning,
                    "请先完成当前编辑（拖动 / 绘制 / 调整大小）后再执行撤销/重做",
                );
            }
            tracing::info!(
                "Editor: 拦截 {:?}（toast_enabled={}, edit_state={:?}）",
                action,
                self.intercept_notification_enabled(),
                self.editor.editor_state.interaction.edit_state
            );
            return false;
        }

        let old_tick = self.editor.playback_position;
        {
            puffin::profile_scope!("editor_handle_action");
            self.editor.handle_action(action);
        }
        // 画布 Ctrl+单击（填充桶）→ 取走弹窗请求，打开「分音符填充」覆盖层。
        // Editor 不持有主窗口对话框状态，只能置一次性标志，由此处转成 UI 状态。
        if self.editor.take_fill_division_dialog_request() {
            self.open_fill_division_dialog();
        }
        let new_tick = self.editor.playback_position;

        // 检查播放位置是否变化
        if (old_tick - new_tick).abs() > f32::EPSILON
            && let Some(manager) = &mut self.playback.manager
        {
            manager.seek(new_tick);
        }

        if is_playhead_or_scroll {
            return false;
        }

        // 检查音符数据是否变化
        let notes_changed = self.editor.notes_changed();
        if notes_changed {
            puffin::profile_scope!("update_playback_notes_on_release");
            self.update_playback_notes();
            self.editor.clear_notes_changed();
        }
        notes_changed
    }

    /// 打开「分音符填充」面板（输入框预填当前档位）
    ///
    /// 触发路径：画布 Ctrl+单击（`Editor` 置请求位 → 此处取走）；
    /// 以及音符画工具箱「颜料桶」条目的 Ctrl+点击（`ToolPanelItemCtrlSelected`）。
    ///
    /// 该面板已从全屏居中弹窗重构为贴图标上方的**工具栏小面板**（由
    /// `root/draw_toolbar.rs` 渲染），故此处需确保音符画工具箱处于展开态——
    /// 否则面板无处锚定；同时与其他工具设置下拉（画刷 / 形状）互斥。
    pub(crate) fn open_fill_division_dialog(&mut self) {
        let current = self.editor.fill_division();
        self.state.fill_division_dialog.is_open = true;
        self.state.fill_division_dialog.value = current.map(|n| n.to_string()).unwrap_or_default();
        self.toolbar.tool_panel_open = true;
        self.toolbar.brush_dropdown_open = false;
        self.toolbar.shape_dropdown_open = false;
    }

    /// 处理钢琴卷帘右键上下文菜单动作
    pub(crate) fn handle_piano_roll_context_menu(&mut self, action: PianoRollContextMenuAction) {
        match action {
            PianoRollContextMenuAction::Open { position, target } => {
                let pos = iced_core::Point::new(position.x, position.y);
                match target {
                    // 图形目标：右键命中哪个图形就作用于它。若它**已在选中集内**则保持
                    // 整个多选不变（菜单「删除」作用于既有批量选区），与音符目标的语义一致。
                    ContextMenuTarget::DrawnShape => {
                        if let Some(id) = self.editor.drawn_shape_at_screen(pos) {
                            self.editor.select_drawn_shape(id);
                        }
                    }
                    // 音符目标：右键点击音符且该音符不在选中集合时，先将其设为唯一选中。
                    // 使菜单的 删除/剪切/复制 作用于"右键目标"，与 Delete 键
                    // "有选中集合即删除选中集合"的语义一致——否则右键一个未选中的
                    // 音符，菜单"删除"会删掉旧的批量选区。
                    ContextMenuTarget::Notes => {
                        if let Some((index, _)) = self.editor.hit_test_note(pos)
                            && !self.editor.is_note_selected(index)
                        {
                            self.editor.selection_clear();
                            self.editor.selection_insert(index);
                        }
                    }
                }
                self.editor.context_menu.open(pos, target);
            }
            PianoRollContextMenuAction::Close => {
                self.editor.context_menu.close();
            }
            PianoRollContextMenuAction::ItemClicked(item) => {
                self.editor.context_menu.close();
                match item {
                    PianoRollContextMenuItem::BatchEdit => {
                        crate::event::emit(crate::event::Event::Window(
                            crate::event::window::Event::open_batch_edit_dialog(),
                        ));
                    }
                    PianoRollContextMenuItem::Cut => {
                        let _ = self.handle_editor_action(EditorAction::Cut);
                    }
                    PianoRollContextMenuItem::Copy => {
                        let _ = self.handle_editor_action(EditorAction::Copy);
                    }
                    PianoRollContextMenuItem::Paste => {
                        let _ = self.handle_editor_action(EditorAction::Paste);
                    }
                    PianoRollContextMenuItem::Delete => {
                        let _ = self.handle_editor_action(EditorAction::DeletePressed);
                    }
                    PianoRollContextMenuItem::DeleteDrawnShape => {
                        // 与 Delete 键同一条路径：鼠标工具下会优先删除选中的绘制图形
                        let _ = self.handle_editor_action(EditorAction::DeletePressed);
                    }
                    PianoRollContextMenuItem::SelectAll => {
                        let _ = self.handle_editor_action(EditorAction::SelectAll);
                    }
                }
            }
        }
    }
}
