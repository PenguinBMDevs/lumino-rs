//! 单 band 的光栅化：直接遍历 `MidiDocument`，不构建任何音符实例副本。
//!
//! 洋葱皮 pass：track 0..N（跳过当前轨），owner=1；主轨 pass：当前轨，owner=2。
//! 稳定深度语义与 GPU 一致：主轨恒覆盖洋葱皮；同级先到先得。
//! 滚动与静止共用同一 Exact 光栅化，**不存在省填充/省描边/丢轨的滚动档位**。

use std::sync::Arc;

use lumino_midi_loader::MidiDocument;

use crate::{
    camera::Camera,
    model::TrackIndex,
    render::{buffer::BandBuffer, chrome, color::NoteStyle, simd::SimdLevel},
};

/// 渲染精度档位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LodMode {
    /// 全质量：完整高度 + 填充 + 2px 描边 + owner 稳定序。
    Exact,
    /// 全曲总览：密度热力图。
    Overview,
}

impl LodMode {
    /// 可读名称。
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Overview => "overview",
        }
    }
}

/// 一条 band 的完整渲染输入。
#[derive(Clone)]
pub struct BandJob {
    /// band 在窗口中的起始 x（含键盘区域偏移）。
    pub x0: u32,
    /// band 结束 x（排他）。
    pub x1: u32,
    /// 相机快照。
    pub camera: Camera,
    /// 精度档位。
    pub mode: LodMode,
    /// 当前音轨。
    pub current_track: usize,
    /// 每轨颜色。
    pub style: Arc<NoteStyle>,
    /// 聚合索引（只读）。
    pub index: Arc<TrackIndex>,
    /// 音符唯一权威源（与编辑层同一个 Arc）。
    pub document: Arc<MidiDocument>,
    /// SIMD 档位。
    pub simd: SimdLevel,
    /// 可见 key 区间。
    pub key_min: u16,
    pub key_max: u16,
}

/// band 渲染统计。
#[derive(Debug, Default, Clone, Copy)]
pub struct BandStats {
    /// 实际尝试绘制的音符数（通过 tick/key 过滤）。
    pub visible_notes: u64,
    /// 窗口迭代到的候选音符数（含被 key/tick 过滤前的）。
    pub candidates: u64,
    /// 长音符 underlay 扫描数。
    pub long_candidates: u64,
}

/// 渲染一条 band。
pub fn render_band(job: &BandJob, buffer: &mut BandBuffer) -> BandStats {
    let width = (job.x1 - job.x0) as usize;
    let height = job.camera.height as usize;
    buffer.ensure(width, height);
    buffer.clear_owner();

    chrome::render_background(
        buffer,
        &job.camera,
        job.x0,
        job.simd,
        job.mode != LodMode::Overview,
    );

    if job.mode == LodMode::Overview {
        // 总览热力图由后续密度金字塔模块接管；先保留背景。
        return BandStats::default();
    }

    render_notes(job, buffer)
}

fn render_notes(job: &BandJob, buffer: &mut BandBuffer) -> BandStats {
    let camera = &job.camera;
    let (tick_lo, tick_hi) = camera.tick_range_for_x(job.x0 as f32, job.x1 as f32);
    let track_count = job.document.track_count();
    let mut stats = BandStats::default();
    let cull_only = std::env::var_os("CPU_ONIONSKIN_CULL_ONLY").is_some();

    // pass 0 = 洋葱皮，pass 1 = 主轨（后画，覆盖洋葱皮）。
    for pass in 0..2u8 {
        for track in 0..track_count {
            let is_main = track == job.current_track;
            if (pass == 1) != is_main {
                continue;
            }
            let notes = job.document.track_notes(track);
            if notes.is_empty() {
                continue;
            }
            // 长音符 underlay：先画，轨内正常音符后画（近似 GPU 的同轨先到先得）。
            for note in job.index.long_notes(track) {
                stats.long_candidates += 1;
                if note.start_tick > tick_hi {
                    break;
                }
                if note.end_tick < tick_lo {
                    continue;
                }
                if (note.key as u16) < job.key_min || (note.key as u16) > job.key_max {
                    continue;
                }
                if cull_only {
                    stats.visible_notes += 1;
                    continue;
                }
                super::notes::draw_note(job, buffer, track, is_main, note, &mut stats);
            }
            let lookback = job.index.lookback(track);
            let (lo, hi) = notes.window_range(tick_lo, tick_hi.saturating_add(1), lookback);
            for (_, note) in notes.iter_window(lo, hi) {
                stats.candidates += 1;
                if note.length() > crate::model::LOOKBACK_TICKS {
                    // 已由 long_notes underlay 覆盖，避免重复绘制。
                    continue;
                }
                if note.end_tick < tick_lo || note.start_tick > tick_hi {
                    continue;
                }
                if (note.key as u16) < job.key_min || (note.key as u16) > job.key_max {
                    continue;
                }
                if cull_only {
                    stats.visible_notes += 1;
                    continue;
                }
                super::notes::draw_note(job, buffer, track, is_main, note, &mut stats);
            }
        }
    }
    stats
}
