//! 编辑器动作与钢琴卷帘上下文菜单处理器
//!
//! 处理 `Message::EditorAction` 与 `Message::PianoRollContextMenu`，
//! 将编辑操作委托给 Editor，并负责播放引擎同步。

use crate::message::EditorAction;
use crate::root::Root;
use lumino_message::{PianoRollContextMenuAction, PianoRollContextMenuItem};

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

    /// 处理钢琴卷帘右键上下文菜单动作
    pub(crate) fn handle_piano_roll_context_menu(&mut self, action: PianoRollContextMenuAction) {
        match action {
            PianoRollContextMenuAction::Open { position } => {
                self.editor
                    .context_menu
                    .open(iced_core::Point::new(position.x, position.y));
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
                    _ => {
                        let editor_action = match item {
                            PianoRollContextMenuItem::Cut => EditorAction::Cut,
                            PianoRollContextMenuItem::Copy => EditorAction::Copy,
                            PianoRollContextMenuItem::Paste => EditorAction::Paste,
                            PianoRollContextMenuItem::Delete => EditorAction::DeletePressed,
                            PianoRollContextMenuItem::SelectAll => EditorAction::SelectAll,
                            PianoRollContextMenuItem::BatchEdit => unreachable!(),
                        };
                        self.handle_editor_action(editor_action);
                    }
                }
            }
        }
    }
}
