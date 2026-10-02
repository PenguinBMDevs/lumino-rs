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
    /// # P1 修复（键盘通、菜单不通）+ 视图仲裁收口
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
    ///
    /// 视图取自 [`crate::root::Root::edit_view`]（唯一权威源），且用**穷尽 match**
    /// 分派：将来新增编辑视图时，此处会编译失败并强制补齐——这就是「新命令不可能
    /// 忘记判视图」的机制保障。
    pub fn handle_edit_menu_action(&mut self, event: EditEvent) {
        use lumino_ui_editor::EditView;

        let (action, arrangement_msg) = match (event, self.root.edit_view()) {
            // 撤销 / 重做不分流：操作全局历史栈，两视图共用
            (EditEvent::Undo, _) => (Some(message::EditorAction::Undo), None),
            (EditEvent::Redo, _) => (Some(message::EditorAction::Redo), None),
            (EditEvent::Cut, EditView::Arrangement) => {
                (None, Some(message::Message::ArrangementCut))
            }
            (EditEvent::Copy, EditView::Arrangement) => {
                (None, Some(message::Message::ArrangementCopy))
            }
            (EditEvent::Paste, EditView::Arrangement) => {
                (None, Some(message::Message::ArrangementPaste))
            }
            (EditEvent::SelectAll, EditView::Arrangement) => {
                (None, Some(message::Message::ArrangementSelectAll))
            }
            (EditEvent::Cut, EditView::PianoRoll) => (Some(message::EditorAction::Cut), None),
            (EditEvent::Copy, EditView::PianoRoll) => (Some(message::EditorAction::Copy), None),
            (EditEvent::Paste, EditView::PianoRoll) => (Some(message::EditorAction::Paste), None),
            (EditEvent::SelectAll, EditView::PianoRoll) => {
                (Some(message::EditorAction::SelectAll), None)
            }
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

    /// 空格键的**视图仲裁**（纯函数，便于测试）：返回本次应发的消息。
    ///
    /// * `clip_panel_active` — 渲染器首级（视频剪辑面板）是否激活。它持有
    ///   **自己的秒域传输时钟**，与卷帘 `PlaybackManager` 完全无关：若无条件切
    ///   卷帘播放，用户在剪辑界面按空格会「什么都没发生」（剪辑时钟根本没被切）。
    ///   因此剪辑面板激活时必须发 `ClipPlayToggled`——与面板上的播放按钮
    ///   **同一条消息**，保证按钮与快捷键行为严格一致（含「停在末尾时从 0 重播」
    ///   等既定语义，不在此处复刻）。
    /// * `piano_roll_playing` — 卷帘走带是否在播放，决定卷帘路径切 Play 还是 Pause。
    /// * `ctrl_or_cmd` — 带 Ctrl/Cmd 的空格**不承接**：Windows 中文输入法切换、
    ///   macOS Spotlight 都占用该组合键，抢过来会在切输入法时莫名开始播放。
    /// * `repeat` — 长按自动重复必须忽略，否则按住空格会以按键重复率疯狂翻转播放态。
    fn space_shortcut_message(
        clip_panel_active: bool,
        piano_roll_playing: bool,
        ctrl_or_cmd: bool,
        repeat: bool,
    ) -> Option<message::Message> {
        if repeat || ctrl_or_cmd {
            return None;
        }
        if clip_panel_active {
            Some(message::Message::VideoClip(
                crate::message::VideoClipAction::ClipPlayToggled,
            ))
        } else if piano_roll_playing {
            Some(message::Message::Toolbar(toolbar::Event::Pause))
        } else {
            Some(message::Message::Toolbar(toolbar::Event::Play))
        }
    }

    /// 处理空格键：播放/暂停切换（按视图分流，见 [`Self::space_shortcut_message`]）
    fn handle_space_shortcut(&mut self, ctrl_or_cmd: bool, repeat: bool) {
        let Some(msg) = Self::space_shortcut_message(
            self.root.is_renderer_entry_active(),
            self.root.toolbar.is_playing,
            ctrl_or_cmd,
            repeat,
        ) else {
            return;
        };
        self.route_message(msg);
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
        repeat: bool,
    ) {
        let ctrl = super::is_ctrl_or_cmd_pressed(modifiers);
        let shift = modifiers.contains(winit::keyboard::ModifiersState::SHIFT);

        // 空格键：播放/暂停切换（视图仲裁 + 忽略长按重复，见 space_shortcut_message）。
        // 无论是否消费都提前返回：空格不参与其余快捷键匹配。
        if key == winit::keyboard::KeyCode::Space {
            self.handle_space_shortcut(ctrl, repeat);
            return;
        }

        // 工程走带视图激活时，先尝试走带快捷键
        // 视图判定统一走 `is_arrangement_mode()`（唯一权威判定入口）——
        // 此前此处裸比 `sidebar.route == Route::Arrangement`，与全仓其他 4 处
        // 判定写法不一致，是「抄错写法」的高发点
        if self.root.is_arrangement_mode()
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
    use crate::message::VideoClipAction;
    use crate::{message, toolbar};
    use winit::keyboard::KeyCode;

    /// Ctrl+S 命中，无 Ctrl 不命中，其他键不命中
    #[test]
    fn test_match_save_shortcut() {
        assert!(Host::match_save_shortcut(KeyCode::KeyS, true));
        assert!(!Host::match_save_shortcut(KeyCode::KeyS, false));
        assert!(!Host::match_save_shortcut(KeyCode::KeyA, true));
        assert!(!Host::match_save_shortcut(KeyCode::Space, true));
    }

    /// 空格键裁决：剪辑面板激活 → 切**剪辑面板独立传输**（与面板播放按钮同一消息）。
    ///
    /// 若这里退回卷帘 Play/Pause，剪辑面板的秒域时钟不会被切换，用户按空格
    /// 会「什么都没发生」——这正是本卡要补的行为。
    #[test]
    fn test_space_shortcut_routes_to_clip_transport_in_clip_panel() {
        let msg = Host::space_shortcut_message(true, false, false, false)
            .expect("剪辑面板内空格应产生消息");
        assert!(
            matches!(
                msg,
                message::Message::VideoClip(VideoClipAction::ClipPlayToggled)
            ),
            "剪辑面板内空格必须切剪辑传输，实际 {msg:?}"
        );

        // 剪辑面板的空格切换与卷帘播放状态无关（两套时钟互不驱动）
        let msg_playing = Host::space_shortcut_message(true, true, false, false)
            .expect("剪辑面板内空格应产生消息");
        assert!(
            matches!(
                msg_playing,
                message::Message::VideoClip(VideoClipAction::ClipPlayToggled)
            ),
            "卷帘正在播放也不得改写剪辑面板的空格语义"
        );
    }

    /// 非剪辑面板视图保持原语义：按卷帘走带状态切 Play / Pause。
    #[test]
    fn test_space_shortcut_keeps_piano_roll_semantics_elsewhere() {
        let idle =
            Host::space_shortcut_message(false, false, false, false).expect("空闲时应发 Play");
        assert!(
            matches!(idle, message::Message::Toolbar(toolbar::Event::Play)),
            "卷帘空闲时空格应播放，实际 {idle:?}"
        );

        let playing =
            Host::space_shortcut_message(false, true, false, false).expect("播放中应发 Pause");
        assert!(
            matches!(playing, message::Message::Toolbar(toolbar::Event::Pause)),
            "卷帘播放中空格应暂停，实际 {playing:?}"
        );
    }

    /// 长按自动重复必须被忽略，否则按住空格会以按键重复率疯狂翻转播放态。
    #[test]
    fn test_space_shortcut_ignores_key_repeat() {
        assert!(
            Host::space_shortcut_message(true, false, false, true).is_none(),
            "剪辑面板内长按空格不得重复切换"
        );
        assert!(
            Host::space_shortcut_message(false, false, false, true).is_none(),
            "卷帘视图长按空格不得重复切换"
        );
    }

    /// 带 Ctrl/Cmd 的空格不承接（Windows 输入法切换 / macOS Spotlight 占用该组合）。
    #[test]
    fn test_space_shortcut_ignores_ctrl_or_cmd() {
        assert!(Host::space_shortcut_message(true, false, true, false).is_none());
        assert!(Host::space_shortcut_message(false, false, true, false).is_none());
        assert!(Host::space_shortcut_message(false, true, true, false).is_none());
    }
}
