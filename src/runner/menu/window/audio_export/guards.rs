//! 音频导出启动前的拒绝守卫。

use crate::runner::RunnerInner;
use lumino_core::tr;
use lumino_message::events::window::audio::AudioBackend;

impl RunnerInner {
    /// 已有导出任务进行中时拒绝新的音频导出，返回 `true` 表示已拒绝。
    pub(super) fn reject_if_export_busy(&mut self) -> bool {
        if self.window_state.audio_export_control.is_some()
            || self
                .window_state
                .video_export_running
                .load(std::sync::atomic::Ordering::SeqCst)
        {
            tracing::warn!("已有导出任务进行中，忽略新的音频导出请求");
            self.window_state
                .window
                .ui_mut()
                .set_status_message(Some(tr!("已有导出任务进行中，请等待完成").to_string()));
            return true;
        }
        false
    }

    /// GPU 丢失/重连期间拒绝 GPU 后端导出，返回 `true` 表示已拒绝。
    pub(super) fn reject_gpu_export_if_device_lost(&mut self, backend: AudioBackend) -> bool {
        if matches!(backend, AudioBackend::Gpu) && lumino_gfx::device_lost() {
            tracing::warn!("GPU 丢失/重连进行中，拒绝启动 GPU 音频导出");
            self.window_state
                .window
                .ui_mut()
                .set_status_message(Some(tr!("GPU 正在恢复，请稍后重试导出").to_string()));
            return true;
        }
        false
    }
}
