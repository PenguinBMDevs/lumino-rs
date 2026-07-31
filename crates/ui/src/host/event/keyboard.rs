//! Host 键盘快捷键处理子模块

use iced_winit::winit;

use crate::host::Host;
use crate::{message, toolbar};
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
                return None;
            }
            _ => None,
        }
    }

    /// 处理 Ctrl+Q：量化弹窗
    fn handle_ctrl_q_shortcut(&mut self) {
        self.route_message(message::Message::Toolbar(toolbar::Event::Quantize));
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
        if self.root.sidebar.route == Route::Arrangement {
            if let Some(msg) = Self::match_arrangement_shortcut(key, ctrl, shift) {
                self.route_message(msg);
                self.window_ctx.window.request_redraw();
                return;
            }
        }

        // Ctrl+Q：单独处理
        if key == winit::keyboard::KeyCode::KeyQ && ctrl {
            self.handle_ctrl_q_shortcut();
            return;
        }

        // 编辑器动作
        if let Some(action) = Self::match_editor_shortcut(key, ctrl, shift) {
            // 通过 Host::handle_action 处理，确保高精度贴图脏标记被正确设置
            self.handle_action(action);
        }
    }
}
