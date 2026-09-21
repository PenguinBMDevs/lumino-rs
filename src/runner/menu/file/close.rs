//! Runner 文件菜单：关闭 / 新建工程与保存确认流程

use winit::event_loop::ActiveEventLoop;

use lumino_ui::message::SaveConfirmAction;
use lumino_ui::state::root_state::DialogType;

use crate::runner::RunnerInner;
use crate::runner::inner::PendingCloseAction;

impl RunnerInner {
    /// 创建新文件
    pub(super) fn handle_new_file(&mut self) {
        // 清空当前工程
        self.midi_state.current_midi = None;
        self.midi_state.current_midi_source = None;
        self.midi_state.cloud_source = None;
        lumino_extras::palette::unlock_palette();

        // 工程级数据随新工程一起归零：编辑计时/创建时间
        // （clear_editor 内部已重置工程设置对话框状态）
        self.session_tracker.reset();

        // 清空编辑器
        self.window_state.window.ui_mut().clear_editor();

        // 恢复主窗口默认标题（工程设置确认时设置的 "{标题} - Lumino"）
        self.window_state.window.window().set_title("Lumino");

        tracing::info!("已创建新工程");
    }

    /// 关闭当前工程（菜单「关闭」）
    ///
    /// 清空编辑器与工程级状态，恢复空白工程与默认标题。
    /// 未保存更改的检查在调用方（`request_close_action`）完成。
    pub(super) fn handle_close_project(&mut self) {
        self.midi_state.current_midi = None;
        self.midi_state.current_midi_source = None;
        self.midi_state.cloud_source = None;
        lumino_extras::palette::unlock_palette();
        self.window_state
            .window
            .ui_mut()
            .dispose_texture_waterfall();
        // 工程级数据必须随工程关闭一起归零：编辑计时/创建时间
        // （clear_editor 内部已重置工程设置对话框状态）
        self.session_tracker.reset();
        self.window_state.window.ui_mut().clear_editor();
        // 恢复主窗口默认标题（工程设置确认时设置的 "{标题} - Lumino"）
        self.window_state.window.window().set_title("Lumino");
        tracing::info!("工程已关闭");
    }

    // ── 保存确认对话框（关闭工程 / 打开另一个工程 / 退出前的未保存更改确认）────

    /// 请求执行一个关闭类动作，必要时先弹出保存确认对话框
    ///
    /// 若工程存在未保存更改，则暂存 `action` 并打开保存确认对话框；
    /// 否则立即执行。保存确认对话框已打开时忽略重复请求（避免堆叠）。
    pub(crate) fn request_close_action(
        &mut self,
        action: PendingCloseAction,
        event_loop: &ActiveEventLoop,
    ) {
        // 保存确认对话框已挂起（或正在初始化）：忽略重复触发，避免堆叠
        if self.pending_close_action.is_some()
            || self
                .window_state
                .dialog_manager
                .has_dialog_type(DialogType::SaveConfirm)
        {
            tracing::debug!("保存确认对话框已挂起，忽略重复关闭请求");
            return;
        }

        if self.window_state.window.ui().is_project_modified() {
            tracing::debug!("工程存在未保存更改，弹出保存确认对话框：{:?}", action);
            self.pending_close_action = Some(action);
            self.window_state
                .dialog_manager
                .open_dialog(DialogType::SaveConfirm);
        } else {
            self.execute_close_action(action, event_loop);
        }
    }

    /// 执行挂起的关闭类动作
    ///
    /// `Exit` / `WindowClose` 在保存进行中时转为 `close_pending`（保存完成后由
    /// `about_to_wait` 退出），否则直接退出事件循环；其余动作立即执行对应处理。
    fn execute_close_action(&mut self, action: PendingCloseAction, event_loop: &ActiveEventLoop) {
        match action {
            PendingCloseAction::CloseProject => self.handle_close_project(),
            PendingCloseAction::NewProject => self.handle_new_file(),
            PendingCloseAction::OpenProject => self.handle_open_file(),
            PendingCloseAction::Exit | PendingCloseAction::WindowClose => {
                // 保存期间禁止关闭：转为待关闭，保存完成后自动退出
                if self.is_saving() || self.is_cloud_saving() {
                    tracing::info!("保存进行中，退出请求延迟到保存完成");
                    self.window_state.window.close_pending = true;
                } else {
                    event_loop.exit();
                }
            }
        }
    }

    /// 处理保存确认对话框结果
    ///
    /// - 保存：记住待执行动作，保存完成后（`handle_save_completed`）继续；
    ///   若保存未实际启动（如空白工程无路径），直接继续避免卡死。
    /// - 关闭（放弃）：执行挂起的动作（Exit/WindowClose 转为 close_pending）。
    /// - 取消：清空挂起动作，继续编辑。
    pub(crate) fn handle_save_confirm_result(&mut self, choice: SaveConfirmAction) {
        match choice {
            SaveConfirmAction::Save => {
                let had_pending = self.pending_close_action.is_some();
                self.run_pending_after_save = had_pending;
                // 执行保存（异步；完成后 handle_save_completed 继续原动作）
                let started = self.handle_save_file();
                if started {
                    // 保存已启动：等待 handle_save_completed 继续挂起的关闭动作
                    tracing::info!("保存确认：已启动保存，等待保存完成后继续关闭动作");
                } else {
                    // 保存未启动（用户取消保存对话框 / 保存进行中被拒绝）：
                    // 视为取消关闭操作，清空挂起状态，绝不强制关闭工程。
                    tracing::info!("保存确认：保存未启动，放弃关闭操作");
                    self.pending_close_action = None;
                    self.run_pending_after_save = false;
                }
            }
            SaveConfirmAction::Discard => {
                // 放弃更改：直接执行挂起的关闭动作（Exit/WindowClose 转为 close_pending）
                self.run_pending_after_save = false;
                self.execute_pending_close_action();
            }
            SaveConfirmAction::Cancel => {
                // 取消关闭操作：清空挂起状态，继续编辑
                self.pending_close_action = None;
                self.run_pending_after_save = false;
                tracing::info!("保存确认：用户取消关闭操作");
            }
        }
    }

    /// 继续（或在放弃/保存完成后执行）挂起的关闭类动作
    ///
    /// 供「放弃更改」与「保存完成后」复用。`Exit` / `WindowClose` 转为 `close_pending`，
    /// 由 `about_to_wait` 在下一帧退出事件循环（无 `event_loop` 上下文时使用此路径）。
    fn execute_pending_close_action(&mut self) {
        let action = match self.pending_close_action.take() {
            Some(action) => action,
            None => return,
        };
        self.run_pending_after_save = false;
        match action {
            PendingCloseAction::CloseProject => self.handle_close_project(),
            PendingCloseAction::NewProject => self.handle_new_file(),
            PendingCloseAction::OpenProject => self.handle_open_file(),
            PendingCloseAction::Exit | PendingCloseAction::WindowClose => {
                self.window_state.window.close_pending = true;
            }
        }
    }

    /// 保存完成后继续挂起的关闭类动作（保存确认对话框选择「保存」时设置）
    pub(super) fn finish_pending_close_action(&mut self) {
        self.execute_pending_close_action();
    }
}
