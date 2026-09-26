//! Host 键盘快捷键处理子模块

use iced_winit::winit;

use crate::host::Host;
use crate::{message, toolbar};
use lumino_ui_core::sidebar_event::Route;

/// 原生菜单栏「编辑」动作的别名（`crate::event::menu::edit::Event`）。
///
/// 单独起名而非在 impl 内 `use`：本文件已有 `use crate::{message, toolbar};`
/// 顶层导入，方法体内的 `use` 会与之形成遮蔽，读代码时容易看错来源。
type EditEvent = crate::event::menu::edit::Event;

impl Host {
    /// 原生菜单栏「编辑」动作的**视图仲裁**入口。
    ///
    /// # P1 修复（键盘通、菜单不通）
    ///
    /// 原生菜单栏的剪切 / 复制 / 粘贴 / 全选此前在 Runner 侧**无条件**路由到
    /// 钢琴卷帘 `EditorAction`，零视图判断。后果：走带视图下同一操作有两条路——
    /// 键盘 Ctrl+C 走 `match_arrangement_shortcut`（通的），菜单栏「复制」走
    /// 卷帘选区（通常是空的，静默无反应）。用户视角就是「同一个功能，键盘能用
    /// 菜单不能用」，必然当成程序 bug。
    ///
    /// 仲裁统一收口在此（UI 层）：Runner 只负责把菜单事件递进来，不该知道
    /// 「卷帘 / 走带」这种视图细节。撤销 / 重做两个动作**不分流**——它们操作的是
    /// 全局历史栈，两视图共用（走带批量操作同样 `push_history`）。
    pub fn handle_edit_menu_action(&mut self, event: EditEvent) {
        let arrangement = self.root.is_arrangement_mode();
        let (action, arrangement_msg) = match (event, arrangement) {
            (EditEvent::Undo, _) => (Some(message::EditorAction::Undo), None),
            (EditEvent::Redo, _) => (Some(message::EditorAction::Redo), None),
            (EditEvent::Cut, true) => (None, Some(message::Message::ArrangementCut)),
            (EditEvent::Copy, true) => (None, Some(message::Message::ArrangementCopy)),
            (EditEvent::Paste, true) => (None, Some(message::Message::ArrangementPaste)),
            (EditEvent::SelectAll, true) => (None, Some(message::Message::ArrangementSelectAll)),
            (EditEvent::Cut, false) => (Some(message::EditorAction::Cut), None),
            (EditEvent::Copy, false) => (Some(message::EditorAction::Copy), None),
            (EditEvent::Paste, false) => (Some(message::EditorAction::Paste), None),
            (EditEvent::SelectAll, false) => (Some(message::EditorAction::SelectAll), None),
            // 「查找」尚未实现（两个视图都无落点），保持原样忽略
            (other, _) => {
                tracing::debug!("Host: 编辑事件 {other:?} 未实现");
                (None, None)
            }
        };

        if let Some(action) = action {
            tracing::info!("Host: 菜单编辑动作 {action:?}（钢琴卷帘路径）");
            self.handle_action(action);
        } else if let Some(msg) = arrangement_msg {
            tracing::info!("Host: 菜单编辑动作 → 工程走带路径 {msg:?}");
            self.process_message(msg);
        }
        self.window_ctx.window.request_redraw();
    }

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
            // P1-5：走带此前无 Ctrl+A，而菜单栏「全选」又不分流 → 用户在走带视图
            // 完全无法全选。补齐后两视图快捷键语义一致。
            (winit::keyboard::KeyCode::KeyA, true, _) => {
                Some(message::Message::ArrangementSelectAll)
            }
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

        // 音轨列表视图（Route::File）下的 Delete/Backspace：已移除删除音轨快捷键，
        // 此处直接拦截返回，避免按键落入编辑器 DeletePressed 造成其他误删。
        if self.root.sidebar.route == Route::File
            && (key == winit::keyboard::KeyCode::Delete
                || key == winit::keyboard::KeyCode::Backspace)
        {
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
