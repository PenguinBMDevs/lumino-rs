//! 对话框事件处理模块

use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use crate::runner::dialog_manager::DialogResult;
use crate::runner::inner::RunnerInner;

impl RunnerInner {
    /// 处理对话框窗口事件
    pub(crate) fn handle_dialog_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) -> bool {
        // 检查是否是对话框窗口
        if !self.window_state.dialog_manager.is_dialog_window(window_id) {
            return false;
        }

        let mut dialog_result = None;
        let mut should_close = false;

        if let Some(dialog) = self.window_state.dialog_manager.get_dialog_mut(window_id) {
            dialog.handle_event(event);

            // 检查对话框是否应该关闭
            should_close = dialog.should_close();

            // 检查对话框结果
            if let Some(result) = dialog.check_result() {
                dialog_result = Some(result);
                should_close = true;
            }
        }

        // 如果应该关闭，关闭对话框
        if should_close {
            self.window_state.dialog_manager.close_dialog(window_id);
        } else {
            // 请求重绘对话框
            if let Some(dialog) = self.window_state.dialog_manager.get_dialog_mut(window_id) {
                dialog.redraw();
            }
        }

        // 处理对话框返回的结果
        if let Some(result) = dialog_result {
            self.process_dialog_result(result);
        }

        true
    }

    /// 处理对话框结果
    fn process_dialog_result(&mut self, result: DialogResult) {
        match result {
            DialogResult::LoadConfirm => {
                if let Some(path) = self.file_state.pending_load_path.take() {
                    self.load_midi_file(path);
                } else {
                    tracing::warn!("LoadConfirm: 没有 pending 的加载路径");
                }
            }
            other => {
                let main_ui = self.window_state.window.ui_mut();
                crate::runner::inner::RunnerInner::apply_dialog_result_to_ui(main_ui, other);
            }
        }
    }
}
