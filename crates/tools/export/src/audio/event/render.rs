//! 帧域批量渲染与采样缓冲池（自 `processor.rs` 拆分，PREF-002）。
//!
//! 渲染循环按块调度：同一块内的事件在块首一次性投递，随后调用一次
//! [`MidiEventProcessor::render_frames`] 整块渲染，替代原"每事件一次引擎
//! 调用"的模式（20M 级素材下调用次数从百万级降到万级）。

use crate::error::ExportResult;
use xsynth_core::AudioPipe;

use super::MidiEventProcessor;

impl<'a> MidiEventProcessor<'a> {
    /// 渲染指定帧数（含暂停/中止检查；启用限幅时逐批处理）。
    ///
    /// - 整数帧域累加，避免旧实现 `(delta 秒 × sr) as usize` 的截断漂移；
    /// - 批量上限 4096 帧，保证暂停/中止的响应粒度与旧实现一致。
    pub(crate) fn render_frames(&mut self, frames: u64) -> ExportResult<()> {
        if frames == 0 {
            return Ok(());
        }
        const MAX_BATCH_FRAMES: u64 = 4096;
        let frame_size = self.channel_count as usize;
        let mut remaining = frames;

        while remaining > 0 {
            if let Some(ctrl) = &self.config.control {
                ctrl.wait_if_paused();
                ctrl.check_abort()?;
            }
            let batch = remaining.min(MAX_BATCH_FRAMES);
            let count = batch as usize * frame_size;

            let mut buffer = self.acquire_buffer(count);
            buffer.resize(count, 0.0);

            // SAFETY: read_samples_unchecked 会填充所有样本
            self.channel_group.read_samples_unchecked(&mut buffer);

            // 应用限制器（如果配置）
            if let Some(limiter) = self.limiter.as_mut() {
                limiter.process(&mut buffer);
            }

            self.sink.write_samples(&buffer)?;
            self.release_buffer(buffer);

            remaining -= batch;
        }

        Ok(())
    }

    /// 从 Vec 池获取或创建新 Vec
    fn acquire_buffer(&mut self, capacity: usize) -> Vec<f32> {
        self.vec_pool
            .pop()
            .unwrap_or_else(|| Vec::with_capacity(capacity))
    }

    /// 归还 Vec 到池
    fn release_buffer(&mut self, buf: Vec<f32>) {
        if self.vec_pool.len() < 4 {
            self.vec_pool.push(buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::MidiEventProcessor;
    use crate::audio::config::AudioRenderConfig;
    use crate::audio::stream::VecSampleSink;
    use crate::audio::tick_conv::TickToTime;
    use xsynth_core::channel_group::ChannelGroup;

    /// 渲染帧数守恒：写入样本数 = 帧数 × 通道数（含跨批次）。
    #[test]
    fn render_frames_writes_exact_sample_count() {
        let config = AudioRenderConfig::default(); // 48kHz / 立体声
        let mut group = ChannelGroup::new(config.build_group_config());
        let mut conv = TickToTime::new(vec![(0, 120.0)], 480);
        let mut sink = VecSampleSink::new();
        {
            let mut processor =
                MidiEventProcessor::new(&config, &mut group, &mut conv, &mut sink);
            processor.render_frames(1000).expect("渲染 1000 帧应成功");
            processor.render_frames(0).expect("0 帧应为空操作");
            processor.render_frames(5000).expect("跨批渲染应成功");
        }
        assert_eq!(sink.len(), 6000 * 2, "应为 6000 帧 × 2 通道");
    }
}
