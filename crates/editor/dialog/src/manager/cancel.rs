//! `DialogManager` 取消请求实现。
//!
//! 与「关闭已就绪窗口」区分：取消还要清掉排队中 / 分帧初始化中的请求，
//! 否则窗口仍会在后续帧创建并弹出来。

use lumino_ui::state::root_state::DialogType;
use winit::window::WindowId;

use super::DialogManager;

impl DialogManager {
    /// 取消指定类型的对话框：排队中、分帧初始化中、已就绪的窗口一并撤掉。
    ///
    /// 用于「打开请求在初始化完成前被撤销」的场景（例如混音台开关被快速点两次）。
    /// 只调 [`Self::mark_dialog_for_close`] 管不到 `pending_dialogs` / `initializing`：
    /// 前者仍会在下一帧建窗，后者已经建好的窗口也会继续初始化并弹出来。
    pub fn cancel_dialog(&mut self, dialog_type: DialogType) {
        self.pending_dialogs
            .retain(|pending| pending.dialog_type != dialog_type);

        // 先整体取出，避免在遍历 `self.initializing` 的同一次借用里调用
        // `self.release_window`（同一结构体的第二个可变借用）。
        let initializing = std::mem::take(&mut self.initializing);
        let mut kept = Vec::with_capacity(initializing.len());
        for (dialog, pending) in initializing {
            if pending.dialog_type == dialog_type {
                // 窗口已建好但 UI 未就绪：随 DialogWindow 一起销毁，归属必须释放。
                self.release_window(dialog.window_id());
            } else {
                kept.push((dialog, pending));
            }
        }
        self.initializing = kept;

        let ids: Vec<WindowId> = self
            .dialogs
            .values()
            .filter(|dialog| dialog.dialog_type() == dialog_type)
            .map(|dialog| dialog.window_id())
            .collect();
        for id in ids {
            self.close_dialog(id);
        }
    }
}
