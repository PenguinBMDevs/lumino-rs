//! Host 键盘快捷键处理子模块

use iced_winit::winit;

use crate::host::Host;
use crate::{message, toolbar};
use lumino_message::TrackContextMenuItem;
use lumino_ui_core::sidebar_event::Route;

impl Host {
    /// 处理空格键：播放/暂停切换
    fn handle_space_shortcut(&mut self) {
        if self.root.toolbar.is_playing {
            self.route_message(message::Message::Toolbar(toolbar::Event::Pause));
        } else {
            self.route_message(message::Message::Toolbar(toolbar::Event::Play));
        }
        self.window_ctx.window.request_redraw();
    }

    /// 匹配工程走带视图快捷键，返回对应的消息
    fn match_arrangement_shortcut(
        key: winit::keyboard::KeyCode,
        ctrl: bool,
        shift: bool,
    ) -> Option<message::Message> {
        match (key, ctrl, shift) {
            (winit::keyboard::KeyCode::Delete | winit::keyboard::KeyCode::Backspace, ..) => {
                Some(message::Message::ArrangementDeleteSelection)
            }
            (winit::keyboard::KeyCode::KeyX, true, _) => Some(message::Message::ArrangementCut),
            (winit::keyboard::KeyCode::KeyC, true, _) => Some(message::Message::ArrangementCopy),
            (winit::keyboard::KeyCode::KeyV, true, _) => Some(message::Message::ArrangementPaste),
            _ => None,
        }
    }

    /// 匹配编辑器动作快捷键，返回对应的 EditorAction
    fn match_editor_shortcut(
        key: winit::keyboard::KeyCode,
        ctrl: bool,
        shift: bool,
    ) -> Option<message::EditorAction> {
        match (key, ctrl, shift) {
            (winit::keyboard::KeyCode::Delete | winit::keyboard::KeyCode::Backspace, ..) => {
                Some(message::EditorAction::DeletePressed)
            }
            (winit::keyboard::KeyCode::KeyZ, true, false) => Some(message::EditorAction::Undo),
            (winit::keyboard::KeyCode::KeyZ, true, true)
            | (winit::keyboard::KeyCode::KeyY, true, _) => Some(message::EditorAction::Redo),
            (winit::keyboard::KeyCode::KeyX, true, _) => Some(message::EditorAction::Cut),
            (winit::keyboard::KeyCode::KeyC, true, _) => Some(message::EditorAction::Copy),
            (winit::keyboard::KeyCode::KeyV, true, _) => Some(message::EditorAction::Paste),
            (winit::keyboard::KeyCode::KeyA, true, _) => Some(message::EditorAction::SelectAll),
            (winit::keyboard::KeyCode::KeyQ, true, _) => {
                // Ctrl+Q 走独立路径（不走 EditorAction）
                None
            }
            _ => None,
        }
    }

    /// 匹配保存快捷键（Ctrl+S / Cmd+S）：返回是否命中
    fn match_save_shortcut(key: winit::keyboard::KeyCode, ctrl: bool) -> bool {
        key == winit::keyboard::KeyCode::KeyS && ctrl
    }

    /// 处理 Ctrl+Q：量化弹窗
    fn handle_ctrl_q_shortcut(&mut self) {
        self.route_message(message::Message::Toolbar(toolbar::Event::Quantize));
    }

    /// 处理音轨列表视图（Route::File）下的 Delete/Backspace 快捷键
    ///
    /// 仅删除当前选中且 `can_delete` 的音轨入口（UI 入口立即移除，
    /// 数据缓存到 `.lmdeltrack` 由 Runner 异步写入）。
    /// Conductor 轨道（can_delete=false）跳过。
    fn handle_track_delete_shortcut(&mut self) {
        let selected_id = self.root.sidebar.selected_track;
        let can_delete = self
            .root
            .sidebar
            .tracks
            .iter()
            .find(|t| t.id == selected_id)
            .map(|t| t.can_delete)
            .unwrap_or(false);
        if !can_delete {
            return;
        }
        self.route_message(
            lumino_ui_core::sidebar_event::Event::track_context_menu_item_clicked(
                selected_id,
                TrackContextMenuItem::Delete,
            ),
        );
        self.window_ctx.window.request_redraw();
    }

    /// 处理键盘快捷键，返回是否有操作
    pub(crate) fn handle_keyboard_shortcuts(
        &mut self,
        key: winit::keyboard::KeyCode,
        modifiers: winit::keyboard::ModifiersState,
    ) {
        let ctrl = super::is_ctrl_or_cmd_pressed(modifiers);
        let shift = modifiers.contains(winit::keyboard::ModifiersState::SHIFT);

        // 空格键：播放/暂停切换
        if key == winit::keyboard::KeyCode::Space {
            self.handle_space_shortcut();
            return;
        }

        // 工程走带视图激活时，先尝试走带快捷键
        if self.root.sidebar.route == Route::Arrangement
            && let Some(msg) = Self::match_arrangement_shortcut(key, ctrl, shift)
        {
            self.route_message(msg);
            self.window_ctx.window.request_redraw();
            return;
        }

        // Ctrl+S：保存工程文件（Runner 侧分流：已有 .lmpj 源则覆盖保存）
        if Self::match_save_shortcut(key, ctrl) {
            crate::event::emit(crate::event::Event::menu_file(
                crate::event::menu::file::Event::save(),
            ));
            return;
        }

        // Ctrl+Q：单独处理
        if key == winit::keyboard::KeyCode::KeyQ && ctrl {
            self.handle_ctrl_q_shortcut();
            return;
        }

        // 音轨列表视图（Route::File）下的 Delete 快捷键：删除选中音轨
        // 与编辑器 Delete（Route 非 File 时由 EditorAction::DeletePressed 处理）互斥。
        if self.root.sidebar.route == Route::File
            && (key == winit::keyboard::KeyCode::Delete
                || key == winit::keyboard::KeyCode::Backspace)
            && !ctrl
        {
            self.handle_track_delete_shortcut();
            return;
        }

        // 编辑器动作
        if let Some(action) = Self::match_editor_shortcut(key, ctrl, shift) {
            // 通过 Host::handle_action 处理，确保高精度贴图脏标记被正确设置
            self.handle_action(action);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Host;
    use winit::keyboard::KeyCode;

    /// Ctrl+S 命中，无 Ctrl 不命中，其他键不命中
    #[test]
    fn test_match_save_shortcut() {
        assert!(Host::match_save_shortcut(KeyCode::KeyS, true));
        assert!(!Host::match_save_shortcut(KeyCode::KeyS, false));
        assert!(!Host::match_save_shortcut(KeyCode::KeyA, true));
        assert!(!Host::match_save_shortcut(KeyCode::Space, true));
    }
}
