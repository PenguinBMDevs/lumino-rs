//! 混音台独立窗口的逐帧对账。
//!
//! 与对话框事件分派分开：这里只做「开关镜像 → 开/关窗」与「主窗口快照 →
//! 对话框 Host」两条同步链路，保持 `dialog.rs` 聚焦事件处理。

use lumino_ui::state::root_state::DialogType;

use crate::runner::inner::RunnerInner;

impl RunnerInner {
    /// 每帧对账混音台独立窗口：开关跟随主窗口镜像，打开期间推送快照。
    ///
    /// 为什么不是「点击时直接开窗」：混音台的开关入口在 `Root` 内（路由栏），
    /// Root 不能创建 OS 窗口；Runner 以主窗口镜像为唯一意图源，每帧对账，
    /// 同时覆盖 `LUMINO_MIXER_OPEN` 调试开关与「用户点 X 关闭窗口」两条路径。
    pub(crate) fn sync_mixer_dialog(&mut self) {
        let desired = self.window_state.window.ui().mixer_open();
        let pending = self
            .window_state
            .dialog_manager
            .is_open_or_pending(DialogType::Mixer);

        if !desired {
            self.mixer_open_requested = false;
            if pending {
                // 用户在窗口初始化完成前再次点路由栏：排队/分帧中的窗口也必须撤掉，
                // 否则会在下一帧「凭空」弹出来。
                self.window_state
                    .dialog_manager
                    .cancel_dialog(DialogType::Mixer);
            }
            return;
        }

        if !pending {
            // 已发过请求但窗口不存在（初始化失败 / 被外部销毁）：不要逐帧重建，
            // 等用户关掉再打开（`desired` 回落时复位）才重试。
            if self.mixer_open_requested {
                return;
            }
            tracing::info!("打开混音台独立窗口");
            self.mixer_open_requested = true;
            self.window_state
                .dialog_manager
                .open_dialog(DialogType::Mixer);
            // 窗口尚未就绪，快照等下一个 `about_to_wait` 再推。
            return;
        }

        let Some(snapshot) = self.window_state.window.ui().mixer_dialog_snapshot() else {
            return;
        };
        let Some(id) = self
            .window_state
            .dialog_manager
            .first_dialog_id_of_type(DialogType::Mixer)
        else {
            return;
        };
        let Some(dialog) = self.window_state.dialog_manager.get_dialog_mut(id) else {
            return;
        };
        if let Some(ui) = dialog.ui_mut()
            && ui.set_mixer_dialog_model(snapshot)
        {
            dialog.request_redraw();
        }
    }

    /// 把混音台窗口本帧产生的动作回放到主窗口。
    ///
    /// 必须在 `DialogManager::update` 之后调用：动作是在对话框重绘时产生的。
    pub(crate) fn drain_mixer_dialog_actions(&mut self) {
        let Some(id) = self
            .window_state
            .dialog_manager
            .first_dialog_id_of_type(DialogType::Mixer)
        else {
            return;
        };
        let actions = match self
            .window_state
            .dialog_manager
            .get_dialog_mut(id)
            .and_then(|dialog| dialog.ui_mut())
        {
            Some(ui) => ui.take_mixer_dialog_actions(),
            None => return,
        };
        if actions.is_empty() {
            return;
        }
        self.window_state
            .window
            .ui_mut()
            .apply_mixer_dialog_actions(actions);
    }
}
