//! REND-019 PR-D：LGS（GPU 合成）独立 device 的丢失与恢复（§1.5 L1/L2）。
//!
//! 关键约束（**红线 8**）：LGS 与编辑器**不共享 device**——本流程独立于主重连
//! 会话（`runner::inner::reconnect`），两者互不等待；主会话字段/步骤不得引用本模块。
//!
//! 链路（L1）：`gpu-synth` device-lost 回调（仅置标志/通知）→ `midi-io`
//! 转发的钩子 → `UserEvent::LgsDeviceLost` → UI 主循环 → 本入口（L2）。
//! 恢复本体在 `MidiManager::request_lgs_rebuild` / `poll_lgs_rebuild`
//! （api 暂持 worker 后台热替换；失败退避重试、耗尽降级提示）。

use super::Runner;

impl Runner {
    /// LGS 独立 device 丢失入口（UI 主循环，经 `UserEvent::LgsDeviceLost`）。
    pub(crate) fn on_lgs_device_lost(&mut self, reason: &str, message: &str) {
        tracing::error!("收到 LgsDeviceLost 用户事件：{reason} — {message}");
        let Some(this) = self.inner.as_mut() else {
            tracing::warn!("初始化未完成，忽略 LGS device-lost");
            return;
        };
        // 一次性中断提示（状态条）：恢复/降级结果随后由 manager 的 notice 覆盖
        this.window_state.window.ui_mut().set_status_message(Some(
            lumino_core::i18n::tr("音频设备丢失，正在恢复 GPU 合成…").to_string(),
        ));
        if this.midi_state.midi.request_lgs_rebuild() {
            tracing::info!("LGS（PR-D）: 重建请求已受理（独立流程；后台热替换）");
        } else {
            tracing::warn!("LGS（PR-D）: 事件被忽略（非 LGS 活跃后端或 API 不可用）");
        }
    }
}
