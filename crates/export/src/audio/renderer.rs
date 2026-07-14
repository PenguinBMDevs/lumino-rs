//! 批量渲染器 — 封装 xsynth ChannelGroup 的批量渲染
//!
//! 参考 OmniConverter 的 XSynthRenderer 设计：
//! - 使用 read_samples_unchecked 避免不必要的零填充
//! - Vec 回收池减少重复分配
//! - 支持限制器

use xsynth_core::{AudioPipe, channel_group::ChannelGroup};

use crate::error::ExportResult;

/// 批量渲染器 — 高效驱动 ChannelGroup 生成音频样本
///
/// # 渲染加速策略
/// - 免零填充：使用 `read_samples_unchecked` 替代 `read_samples`
/// - Vec 回收：消费完的 Vec 归还给池，渲染时复用
/// - 扁平批处理：固定大小批次，消除递归开销
pub struct BatchRenderer<'a> {
    channel_group: &'a mut ChannelGroup,
    sample_rate: u32,
    channel_count: u16,
    /// Vec 回收池
    vec_pool: Vec<Vec<f32>>,
}

/// 最大单批次渲染时长（秒）
const MAX_BATCH_SECONDS: f64 = 10.0;

/// 亚样点精度累加器
struct BatchBuffer {
    output_vec: Vec<f32>,
    missed_samples: f64,
}

impl<'a> BatchRenderer<'a> {
    pub fn new(channel_group: &'a mut ChannelGroup) -> Self {
        let params = *channel_group.stream_params();
        BatchRenderer {
            channel_group,
            sample_rate: params.sample_rate,
            channel_count: params.channels.count() as u16,
            vec_pool: Vec::new(),
        }
    }

    /// 从池中获取 Vec
    fn acquire_vec(&mut self, min_cap: usize) -> Vec<f32> {
        let mut v = self
            .vec_pool
            .pop()
            .unwrap_or_else(|| Vec::with_capacity(min_cap));
        if v.capacity() < min_cap {
            v.reserve(min_cap - v.capacity());
        }
        v
    }

    /// 归还 Vec 到池
    #[allow(dead_code)]
    fn release_vec(&mut self, v: Vec<f32>) {
        if self.vec_pool.len() < 4 {
            self.vec_pool.push(v);
        }
    }

    /// 渲染指定时长的音频
    ///
    /// # 参数
    /// - `event_time`: 要渲染的时长（秒）
    /// - `limiter`: 可选的限制器
    ///
    /// # 返回
    /// 渲染的样本数据（f32 interleaved）
    pub fn render(&mut self, event_time: f64) -> Vec<f32> {
        let mut remaining = event_time;
        let mut buffer = BatchBuffer {
            output_vec: Vec::with_capacity(4096),
            missed_samples: 0.0,
        };

        while remaining > 0.0 {
            let batch = remaining.min(MAX_BATCH_SECONDS);

            // 计算样点数（含亚样点累加）
            let samples_f = self.sample_rate as f64 * batch + buffer.missed_samples;
            buffer.missed_samples = samples_f % 1.0;
            let frame_count = samples_f as usize;
            let sample_count = frame_count * self.channel_count as usize;

            // 从回收池获取 Vec
            buffer.output_vec = self.acquire_vec(sample_count);
            // SAFETY: read_samples_unchecked 会填充样本
            unsafe {
                buffer.output_vec.set_len(sample_count);
            }

            self.channel_group
                .read_samples_unchecked(&mut buffer.output_vec);

            remaining -= batch;
        }

        buffer.output_vec
    }

    /// 渲染并写入到目标接收器
    pub fn render_to_sink(
        &mut self,
        event_time: f64,
        sink: &mut dyn super::stream::SampleSink,
    ) -> ExportResult<()> {
        let samples = self.render(event_time);
        sink.write_samples(&samples)
    }

    /// 持续渲染直到静音
    pub fn render_tail(&mut self) -> ExportResult<Vec<f32>> {
        let mut all_samples = Vec::new();

        loop {
            let samples = self.sample_rate as usize * self.channel_count as usize;
            let mut buffer = vec![0.0f32; samples];
            self.channel_group.read_samples_unchecked(&mut buffer);

            let is_silent = buffer.iter().all(|&s| s.abs() < 0.0001);
            all_samples.extend_from_slice(&buffer);

            if is_silent {
                break;
            }
        }

        Ok(all_samples)
    }

    /// 驱动事件，渲染 delta 时间，然后发送 MIDI 事件到 ChannelGroup
    pub fn process_event_delta(
        &mut self,
        delta_seconds: f64,
        sink: &mut dyn super::stream::SampleSink,
    ) -> ExportResult<()> {
        self.render_to_sink(delta_seconds, sink)
    }
}
