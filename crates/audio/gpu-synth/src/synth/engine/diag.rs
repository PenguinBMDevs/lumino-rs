//! 状态复位与诊断入口（从 `init.rs` 拆出，见 AGENTS.md 的文件长度约定）。

use super::*;

impl GpuSynth {
    /// 清空实时合成状态（文档切换、同布局轻量复位；REND-002 实时多端口）。
    ///
    /// 对齐 XSynth `reset_channel_state` 语义：清通道控制器/程序/弯音状态与
    /// 全部声部/事件队列，保留引擎、音频流与已上传采样（不重建、不重开设备）。
    pub fn reset_channel_state(&mut self) {
        for c in &mut self.channels {
            *c = ChannelState::new();
        }
        // REF-LGS-010: 声部清空的同时回收静态参数槽位。
        self.clear_voices();
        for q in self.key_voices.iter_mut() {
            q.clear();
        }
        self.spawn_budget.fill(0);
        self.active_notes.fill(0);
        self.pending_events.clear();
        self.pending_mix_events.clear();
        self.offline_events.clear();
        self.offline_cursor = 0;
        self.active_voice_count = 0;
        self.prev_voice_ids.clear();
        self.last_out = None;
        self.last_states = None;
        self.pending = None;
        self.voice_templates.clear();
        self.limiter_gain = 1.0;
        self.limiter_tail.clear();
    }

    /// Returns the number of currently active voices.
    pub fn voice_count(&self) -> usize {
        self.voices.len()
    }

    /// 采样数据内存占用 `(GPU 字节, CPU 字节)`（REF-LGS-013）。
    ///
    /// GPU = `samples_chunks` 实际分配字节；CPU = 原生样本 + 重采样缓存。
    /// 会遍历样本表，供实时统计低频发布，不应每块调用。
    pub fn sample_memory_bytes(&self) -> (u64, u64) {
        let (gpu, cpu, _) = self.sample_memory_breakdown();
        (gpu, cpu)
    }

    /// 采样内存拆分 `(GPU, CPU 合计, CPU 重采样缓存)`（REF-DATA-001）。
    ///
    /// CPU 合计 = 原生 + 重采样（同率别名已去重），重采样缓存是其中的子集。
    pub fn sample_memory_breakdown(&self) -> (u64, u64, u64) {
        let gpu: u64 = self.samples_chunks.iter().map(GrowableBuffer::size).sum();
        let (native, resample) = self
            .sf
            .as_ref()
            .map_or((0, 0), SoundFont::memory_bytes_split);
        (gpu, native.saturating_add(resample), resample)
    }

    /// REF-DATA-001：释放采样池（换音源 / 卸载时调用）。
    ///
    /// 清空采样偏移与游标，并把 4 个 chunk 重建为初始容量（旧缓冲由 wgpu
    /// 在在途提交完成后回收）。**调用方必须先清空声部**：旧声部的
    /// `sample_offset` 会指向新音源的数据；`load_soundfont`/`unload_soundfont`
    /// 内部都先走 `reset_channel_state`。
    pub(crate) fn reset_sample_pool(&mut self) {
        self.sample_offsets.clear();
        self.samples_next_offset = 0;
        let device = &self.res.ctx.device;
        let queue = &self.res.ctx.queue;
        self.samples_chunks = (0..SAMPLES_CHUNKS)
            .map(|i| {
                GrowableBuffer::with_max_capacity(
                    device,
                    queue,
                    &format!("samples chunk {i}"),
                    1 << 20,
                    SAMPLES_CHUNK_BYTES,
                    wgpu::BufferUsages::STORAGE,
                )
            })
            .collect();
        self.render_bg_dirty = true;
    }

    /// Diagnostics: `(voices, released, ended)` - how many voices exist,
    /// how many have a release scheduled, and how many the GPU marked ended.
    #[doc(hidden)]
    pub fn debug_voice_lifecycle(&self) -> (usize, usize, usize) {
        let released = self
            .voices
            .iter()
            .filter(|v| v.released || v.release_at != u64::MAX)
            .count();
        let ended = self.voices.iter().filter(|v| v.state.ended != 0).count();
        (self.voices.len(), released, ended)
    }

    /// Diagnostics: details of the first voice's GPU state.
    #[doc(hidden)]
    pub fn debug_voice_state(&self) -> Option<(u32, u32, u32, u32, u64, u64)> {
        let v = self.voices.first()?;
        Some((
            v.state.is_released,
            v.state.ended,
            v.state.env_stage,
            v.state.env_t,
            v.release_at,
            v.start_at,
        ))
    }

    /// Diagnostics: per-voice `(key, vel, speed, amp, released, ended,
    /// env_stage, env_t, release_at, gpu_is_released, env_from)`.
    #[doc(hidden)]
    pub fn debug_voices(&self) -> Vec<VoiceDebugInfo> {
        self.voices
            .iter()
            .map(|v| {
                (
                    v.key,
                    v.vel,
                    v.speed,
                    v.amp,
                    v.released || v.release_at != u64::MAX,
                    v.state.ended != 0,
                    v.state.env_stage,
                    v.state.env_t,
                    v.release_at,
                    v.state.is_released,
                    v.state.env_from,
                )
            })
            .collect()
    }

    /// The number of frames rendered so far.
    pub fn rendered_frames(&self) -> u64 {
        self.global_frame
    }
}
