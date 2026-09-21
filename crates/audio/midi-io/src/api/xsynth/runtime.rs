use crate::realtime::StreamRestartError;

use super::{XSynth, XSynthStats};

impl XSynth {
    /// 获取运行时统计信息
    pub fn stats(&self) -> XSynthStats {
        let stats = self.synth.get_stats();
        XSynthStats {
            voice_count: stats.voice_count(),
            average_renderer_load: stats.buffer().average_renderer_load(),
            buffer_samples: stats.buffer().last_samples_after_read(),
            event_queue_depth: stats.event_queue_depth(),
            event_queue_high_water: stats.event_queue_high_water(),
            emergency_dropped_notes: stats.emergency_dropped_notes(),
        }
    }

    /// 检查音频流是否因设备移除等不可用，需要恢复。
    ///
    /// 底层（xsynth-realtime）在音频设备被拔出/更换时自动尝试重定向到
    /// 系统默认输出设备；仅当自愈失败（如新设备参数与管线不一致）时才返回 `true`。
    pub fn poll_stream_recovery_needed(&self) -> bool {
        self.synth.poll_recovery_error().is_some()
    }

    /// 恢复音频流：优先直接重定向到系统默认输出设备（合成管线不变），
    /// 重定向不可行（设备参数变化）时全量重建合成管线。
    pub fn recover_stream(&mut self) -> Result<(), String> {
        match self.synth.restart_stream() {
            Ok(()) => {
                tracing::info!("XSynth: 音频流已重定向到默认输出设备（合成管线保持不变）");
                Ok(())
            }
            Err(StreamRestartError::ConfigChanged(msg)) => {
                tracing::warn!("XSynth: 设备参数已改变 ({msg})，重建合成管线");
                self.rebuild()
            }
            Err(e) => {
                tracing::warn!("XSynth: 音频流重定向失败 ({e})，重建合成管线");
                self.rebuild()
            }
        }
    }
}
