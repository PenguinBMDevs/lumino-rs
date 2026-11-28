//! 单 band 的光栅化：直接遍历 `MidiDocument`，不构建任何音符实例副本。
//!
//! 洋葱皮 pass：track 0..N（跳过当前轨），owner=1；主轨 pass：当前轨，owner=2。
//! 稳定深度语义与 GPU 一致：主轨恒覆盖洋葱皮；同级先到先得。
//! 滚动与静止共用同一 Exact 光栅化，**不存在省填充/省描边/丢轨的滚动档位**。

use std::sync::Arc;

use lumino_midi_loader::{MidiDocument, NoteEvent};

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
    /// 彩色覆盖光栅：真实轨道色 + 音符宽度，单元 ≤2px。
    Coverage,
    /// 兜底：全曲密度热力图（仅覆盖不可用且 ≥30 小节）。
    Overview,
}

impl LodMode {
    /// 可读名称。
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Coverage => "coverage",
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
    /// 是否允许"画布已覆盖"提前退出（测试可关闭以做逐像素对照）。
    pub early_coverage: bool,
    /// 是否使用列主序绘制窄音符。
    pub column_major: bool,
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
    /// 提前退出所需覆盖的像素目标。
    pub coverage_target: usize,
    /// 实际覆盖的像素数。
    pub covered: usize,
}

/// 渲染一条 band。
pub fn render_band(job: &BandJob, buffer: &mut BandBuffer) -> BandStats {
    crate::profiling::tracy_zone!("cpu_onion.render_band", {
        let width = (job.x1 - job.x0) as usize;
        let height = job.camera.height as usize;
        buffer.ensure(width, height);
        buffer.clear_owner();

        crate::profiling::tracy_zone!("cpu_onion.background", {
            chrome::render_background(
                buffer,
                &job.camera,
                job.x0,
                job.simd,
                job.mode == LodMode::Exact,
            );
        });

        if job.mode != LodMode::Exact {
            // 覆盖/热力图由 density 模块整帧接管；这里只保留背景。
            return BandStats::default();
        }

        render_notes(job, buffer)
    })
}

/// 单 band 音符遍历的窗口/覆盖参数（pass 的前置状态）。
#[derive(Debug)]
struct PassSetup {
    /// 窗口 tick 下界。
    tick_lo: u32,
    /// 窗口 tick 上界。
    tick_hi: u32,
    /// 文档轨道总数。
    track_count: usize,
    /// 是否只统计剔除结果而不落笔。
    cull_only: bool,
    /// 是否列主序绘制窄音符。
    column_major: bool,
    /// 标尺占用的行数（不计入覆盖统计）。
    ruler_rows: usize,
    /// 提前退出所需覆盖的像素目标。
    coverage_target: usize,
}

impl PassSetup {
    /// 从 band 输入计算窗口、可见行数与覆盖目标。
    fn new(job: &BandJob, buffer: &BandBuffer) -> Self {
        let camera = &job.camera;
        let (tick_lo, tick_hi) = camera.tick_range_for_x(job.x0 as f32, job.x1 as f32);
        let track_count = job.document.track_count();
        let cull_only = std::env::var_os("CPU_ONIONSKIN_CULL_ONLY").is_some();
        let column_major = job.column_major;
        let ruler_rows = (camera.ruler_height.round().max(0.0) as usize).min(buffer.height);
        let canvas_rows = buffer.height.saturating_sub(ruler_rows);
        let key_lo = u32::from(job.key_min.min(127));
        let key_hi = u32::from(job.key_max.min(127));
        let visible_key_mask = if key_hi >= key_lo {
            let span = key_hi - key_lo + 1;
            if span >= 128 {
                u128::MAX
            } else {
                ((1u128 << span) - 1) << key_lo
            }
        } else {
            0
        };
        let active_keys = job
            .index
            .active_keys(tick_lo, tick_hi, job.current_track, &[])
            & visible_key_mask;
        let active_rows = if job.index.key_mask_bytes() == 0 {
            canvas_rows
        } else {
            (active_keys.count_ones() as usize)
                .saturating_mul(camera.zoom_y.ceil().max(1.0) as usize)
                .min(canvas_rows)
        };
        let early_coverage = job.early_coverage;
        let coverage_target = if early_coverage {
            buffer.width.saturating_mul(active_rows)
        } else {
            usize::MAX
        };
        Self {
            tick_lo,
            tick_hi,
            track_count,
            cull_only,
            column_major,
            ruler_rows,
            coverage_target,
        }
    }
}

/// 单音符绘制的相位开关：主轨标记、覆盖统计与标尺行。
#[derive(Debug, Clone, Copy)]
struct NotePhase {
    /// 是否主轨（owner=2）。
    is_main: bool,
    /// 是否统计覆盖像素（洋葱皮 pass）。
    count_coverage: bool,
    /// 是否列主序绘制窄音符。
    column_major: bool,
    /// 标尺占用的行数。
    ruler_rows: usize,
}

fn render_notes(job: &BandJob, buffer: &mut BandBuffer) -> BandStats {
    let pass = PassSetup::new(job, buffer);
    let mut stats = BandStats::default();
    render_note_passes(job, buffer, &mut stats, &pass);
    if pass.column_major {
        buffer.composite_columns();
    }
    stats.coverage_target = pass.coverage_target;
    stats.covered = buffer.covered_pixels;
    stats
}

/// 两趟轨道遍历：pass 0 洋葱皮、pass 1 主轨（后画覆盖洋葱皮）。
fn render_note_passes(
    job: &BandJob,
    buffer: &mut BandBuffer,
    stats: &mut BandStats,
    pass: &PassSetup,
) {
    // pass 0 = 洋葱皮，pass 1 = 主轨（后画，覆盖洋葱皮）。
    for pass_index in 0..2u8 {
        // 洋葱皮已经把画布铺满：同级先到先得，后续洋葱皮轨不可能再改变
        // 任何像素，直接跳到主轨 pass。
        if pass_index == 0 && buffer.covered_pixels >= pass.coverage_target {
            continue;
        }
        'tracks: for track in 0..pass.track_count {
            let is_main = track == job.current_track;
            if (pass_index == 1) != is_main {
                continue;
            }
            let notes = job.document.track_notes(track);
            if notes.is_empty() {
                continue;
            }
            let count_coverage = pass_index == 0;
            let phase = NotePhase {
                is_main,
                count_coverage,
                column_major: pass.column_major,
                ruler_rows: pass.ruler_rows,
            };
            // 长音符 underlay：先画，轨内正常音符后画（近似 GPU 的同轨先到先得）。
            for note in job.index.long_notes(track) {
                stats.long_candidates += 1;
                if note.start_tick > pass.tick_hi {
                    break;
                }
                if note.end_tick < pass.tick_lo {
                    continue;
                }
                if (note.key as u16) < job.key_min || (note.key as u16) > job.key_max {
                    continue;
                }
                if pass.cull_only {
                    stats.visible_notes += 1;
                    continue;
                }
                let covered = draw_note_in_phase(job, buffer, stats, note, track, phase);
                buffer.covered_pixels += covered;
                if count_coverage && buffer.covered_pixels >= pass.coverage_target {
                    break 'tracks;
                }
            }
            let lookback = job.index.lookback(track);
            let (lo, hi) =
                notes.window_range(pass.tick_lo, pass.tick_hi.saturating_add(1), lookback);
            for (_, note) in notes.iter_window(lo, hi) {
                stats.candidates += 1;
                if note.length() > crate::model::LOOKBACK_TICKS {
                    // 已由 long_notes underlay 覆盖，避免重复绘制。
                    continue;
                }
                if note.end_tick < pass.tick_lo || note.start_tick > pass.tick_hi {
                    continue;
                }
                if (note.key as u16) < job.key_min || (note.key as u16) > job.key_max {
                    continue;
                }
                if pass.cull_only {
                    stats.visible_notes += 1;
                    continue;
                }
                let covered = draw_note_in_phase(job, buffer, stats, note, track, phase);
                buffer.covered_pixels += covered;
                if count_coverage && buffer.covered_pixels >= pass.coverage_target {
                    break 'tracks;
                }
            }
        }
    }
}

/// 按列主序/行主序把单个音符画进 band 缓冲，返回新增覆盖像素数。
fn draw_note_in_phase(
    job: &BandJob,
    buffer: &mut BandBuffer,
    stats: &mut BandStats,
    note: &NoteEvent,
    track: usize,
    phase: NotePhase,
) -> usize {
    if phase.column_major {
        super::notes::draw_note_column(
            job,
            buffer,
            track,
            phase.is_main,
            note,
            stats,
            phase.count_coverage,
            phase.ruler_rows,
        )
    } else {
        super::notes::draw_note(
            job,
            buffer,
            track,
            phase.is_main,
            note,
            stats,
            phase.count_coverage,
            phase.ruler_rows,
        )
    }
}
