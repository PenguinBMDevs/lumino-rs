//! 两条 CPU 渲染循环共用的骨架
//!
//! 自 `render_loops.rs` 拆出（REF-001：单文件 ≤ 400 行）：
//! - [`ProgressPump`]：100ms 节流上报、倍速采样与事件/音符计数；
//! - [`RenderDriver`]：块式游标推进、事件投递与收尾（含端口折叠告警）。

use std::time::{Duration, Instant};

use midly::TrackEventKind;

use lumino_midi_model::multi_port::MAX_PORTS;

use crate::error::ExportResult;

use super::block_scheduler::RenderCursor;
use super::config::AudioRenderConfig;
use super::event::MidiEventProcessor;
use super::report::report_progress;
use super::speed::{DEFAULT_SPEED_WINDOW_SECS, ExportSpeedMeter};

/// 进度泵：100ms 节流上报、倍速采样与事件/音符计数（两条 CPU 循环共用）。
///
/// 「事件计数」的口径由调用方决定（流式只数 NoteOn/NoteOff，内存数全部可投递
/// 事件）——泵只保证**累加与收尾上报**单点实现。
pub(super) struct ProgressPump<'a> {
    config: &'a AudioRenderConfig,
    total_ticks: u64,
    total_seconds: f64,
    start_time: Instant,
    last_progress_time: Instant,
    speed_meter: ExportSpeedMeter,
    event_count: u64,
    note_count: u64,
}

impl<'a> ProgressPump<'a> {
    pub(super) fn new(config: &'a AudioRenderConfig, total_ticks: u64, total_seconds: f64) -> Self {
        let now = Instant::now();
        Self {
            config,
            total_ticks,
            total_seconds,
            start_time: now,
            last_progress_time: now,
            speed_meter: ExportSpeedMeter::new(DEFAULT_SPEED_WINDOW_SECS),
            event_count: 0,
            note_count: 0,
        }
    }

    /// 每个事件先做暂停/中止检查，再按 100ms 节流上报进度。
    pub(super) fn on_event(&mut self, tick: u64) -> ExportResult<()> {
        if let Some(ctrl) = &self.config.control {
            ctrl.wait_if_paused();
            ctrl.check_abort()?;
        }
        let now = Instant::now();
        if now.duration_since(self.last_progress_time) >= Duration::from_millis(100) {
            let pct = tick as f64 / self.total_ticks as f64;
            self.speed_meter.record(
                now.duration_since(self.start_time).as_secs_f64(),
                pct * self.total_seconds,
            );
            report_progress(
                self.config,
                pct,
                self.event_count,
                self.note_count,
                self.start_time,
                self.speed_meter.speed(),
            );
            self.last_progress_time = now;
        }
        Ok(())
    }

    /// 计入一个已投递事件。
    pub(super) fn count_event(&mut self) {
        self.event_count += 1;
    }

    /// 计入一个 NoteOn（只加音符数；事件数由 [`Self::count_event`] 负责）。
    pub(super) fn count_note(&mut self) {
        self.note_count += 1;
    }

    /// 收尾：按 100% 上报最终进度，返回（事件数, 音符数）供完成日志使用。
    pub(super) fn finish(self) -> (u64, u64) {
        report_progress(
            self.config,
            1.0,
            self.event_count,
            self.note_count,
            self.start_time,
            self.speed_meter.speed(),
        );
        (self.event_count, self.note_count)
    }
}

/// 事件投递骨架：块式游标推进 → 渲染 → 端口折叠计数（两条 CPU 循环共用）。
pub(super) struct RenderDriver<'p, 'a> {
    processor: &'p mut MidiEventProcessor<'a>,
    cursor: RenderCursor,
    clamped_events: u64,
}

impl<'p, 'a> RenderDriver<'p, 'a> {
    pub(super) fn new(processor: &'p mut MidiEventProcessor<'a>, block_frames: u64) -> Self {
        Self {
            processor,
            cursor: RenderCursor::new(block_frames),
            clamped_events: 0,
        }
    }

    /// 投递一个事件：先按块游标推进时间，再分发并吞入本次额外渲染帧数。
    ///
    /// B1 口径：只统计 MIDI 事件的端口折叠，meta/文本等不计入告警。
    pub(super) fn dispatch(
        &mut self,
        tick: u64,
        kind: &TrackEventKind<'_>,
        port: u8,
    ) -> ExportResult<()> {
        let frame = self.processor.frame_at_tick(tick);
        let advance = self.cursor.frames_before(frame);
        if advance > 0 {
            self.processor.render_frames(advance)?;
        }
        if port >= MAX_PORTS && matches!(kind, TrackEventKind::Midi { .. }) {
            self.clamped_events += 1;
        }
        self.cursor
            .add_rendered(self.processor.dispatch_event(kind, port)?);
        Ok(())
    }

    /// 收尾：补齐到最后一个事件的精确帧（输出长度与逐事件旧实现一致），
    /// 并汇总超上限端口的折叠告警。
    pub(super) fn finish(&mut self, source: &str) -> ExportResult<()> {
        let remainder = self.cursor.finish_remainder();
        if remainder > 0 {
            self.processor.render_frames(remainder)?;
        }
        if self.clamped_events > 0 {
            tracing::warn!(
                "[REND-002] {source}：{} 个事件来自超上限端口，已折叠到端口 {} 块发送（B1）",
                self.clamped_events,
                MAX_PORTS - 1
            );
        }
        Ok(())
    }
}
