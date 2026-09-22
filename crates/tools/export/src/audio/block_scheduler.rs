//! 块式渲染游标 —— 把"事件目标帧"折算为"先渲染多少帧"（PREF-002）。
//!
//! # 背景
//!
//! CPU 导出原本逐事件驱动渲染：每个事件都调用一次合成引擎（16 通道 × 128 键
//! 簿记、声部维护、并行任务调度、`load_program`）。20M 级素材上"每次调用的
//! 固定开销"远大于实际 DSP，且随密度增长而恶化。
//!
//! 块式渲染把事件按固定帧窗（B 帧）聚合：同一块内的事件在块首一次性投递，
//! 随后整块渲染一次，调用次数从"事件数（百万级）"降到
//! "音频秒数 × 采样率 / B（万级）"，与 GPU 后端的 512 帧块式 `apply_events`
//! 对齐。
//!
//! # 语义
//!
//! - 块内事件最多**提前 B 帧**生效（量化到块首），与 GPU 的块式量化行为一致；
//! - `B ∈ {0, 1}` 退化为逐事件精确渲染（旧行为，供 A/B 对照与
//!   `note_force_end_delay` 语义保真使用）；
//! - 渲染总量始终补齐到"最后一个事件的精确帧"，保证输出长度与旧实现一致
//!   （尾部的衰减渲染仍由 `finalize` 负责）。

/// 块式渲染游标：跟踪已渲染帧数，并把事件目标帧折算为需先渲染的帧数。
#[derive(Debug, Clone, Copy)]
pub(crate) struct RenderCursor {
    /// 块大小（帧）；`0`/`1` = 逐事件精确模式
    block: u64,
    /// 已渲染的总帧数（含 [`Self::add_rendered`] 追加的帧）
    rendered: u64,
    /// 最后一个事件的精确帧（收尾补齐用）
    last_event_frame: u64,
}

impl RenderCursor {
    /// 创建游标；`block` 取 `0`/`1` 时表示逐事件精确模式。
    pub(crate) fn new(block: u64) -> Self {
        Self {
            block,
            rendered: 0,
            last_event_frame: 0,
        }
    }

    /// 记录一个位于 `frame` 的事件，返回投递该事件前需渲染的帧数。
    ///
    /// 块模式：目标为 `frame` 所在块的**块首**（向下对齐），块内重复调用返回 0；
    /// 精确模式：目标为 `frame` 本身。事件帧回退（理论上不应发生）时返回 0。
    pub(crate) fn frames_before(&mut self, frame: u64) -> u64 {
        self.last_event_frame = frame;
        let target = if self.block <= 1 {
            frame
        } else {
            (frame / self.block) * self.block
        };
        let advance = target.saturating_sub(self.rendered);
        self.rendered += advance;
        advance
    }

    /// 记录额外渲染的帧数（如 `note_force_end_delay` 的延长渲染）。
    pub(crate) fn add_rendered(&mut self, frames: u64) {
        self.rendered = self.rendered.saturating_add(frames);
    }

    /// 事件流结束后的收尾补齐：渲染到"最后一个事件的精确帧"所需帧数。
    pub(crate) fn finish_remainder(&self) -> u64 {
        self.last_event_frame.saturating_sub(self.rendered)
    }

    /// 已渲染帧数（测试/诊断用）。
    #[cfg(test)]
    pub(crate) fn rendered(&self) -> u64 {
        self.rendered
    }
}

#[cfg(test)]
mod tests {
    use super::RenderCursor;

    /// 块模式下事件量化到块首：同一块内只渲染一次，且发生在块首。
    #[test]
    fn block_alignment_quantizes_events_to_block_start() {
        let mut c = RenderCursor::new(256);
        assert_eq!(c.frames_before(0), 0);
        assert_eq!(c.frames_before(100), 0, "同块内事件不应推进渲染");
        assert_eq!(c.frames_before(255), 0);
        assert_eq!(c.frames_before(256), 256, "跨块时才渲染前一块");
        assert_eq!(c.frames_before(257), 0);
        assert_eq!(c.frames_before(511), 0);
        assert_eq!(c.frames_before(512), 256);
        assert_eq!(c.rendered(), 512);
    }

    /// 精确模式（B=0/1）逐事件推进，等价旧行为。
    #[test]
    fn exact_mode_advances_to_event_frame() {
        let mut c = RenderCursor::new(1);
        assert_eq!(c.frames_before(1000), 1000);
        assert_eq!(c.frames_before(1000), 0, "重复帧不推进");
        assert_eq!(c.frames_before(1005), 5);
        assert_eq!(c.rendered(), 1005);

        let mut z = RenderCursor::new(0);
        assert_eq!(z.frames_before(300), 300);
    }

    /// 回退帧与重复帧永不推进渲染（防御性，保证采样时钟单调）。
    #[test]
    fn backward_frames_never_render() {
        let mut c = RenderCursor::new(64);
        assert_eq!(c.frames_before(500), 448);
        assert_eq!(c.frames_before(100), 0, "回退帧不渲染");
        assert_eq!(c.rendered(), 448);
    }

    /// 收尾补齐到最后一个事件的精确帧（输出长度与旧实现一致）。
    #[test]
    fn finish_remainder_catches_up_to_last_event_frame() {
        let mut c = RenderCursor::new(256);
        assert_eq!(c.frames_before(300), 256);
        assert_eq!(c.frames_before(400), 0);
        assert_eq!(c.frames_before(1000), 512); // 768 - 256
        assert_eq!(c.rendered(), 768);
        assert_eq!(c.finish_remainder(), 232, "补齐到 1000 帧");
    }

    /// `note_force_end_delay` 追加渲染需计入采样时钟，避免重复渲染。
    #[test]
    fn extra_rendered_frames_are_counted() {
        let mut c = RenderCursor::new(1);
        assert_eq!(c.frames_before(300), 300);
        c.add_rendered(48); // 例如 1ms @48k 的延长渲染
        assert_eq!(c.rendered(), 348);
        assert_eq!(c.frames_before(301), 0, "已在 348 帧之后，不再补渲染");
        assert_eq!(c.finish_remainder(), 0, "精确帧已被额外渲染覆盖");
    }
}
