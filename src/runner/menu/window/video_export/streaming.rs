//! 视频导出流式 MIDI 数据源
//!
//! 当内存中没有 MidiDocument 时，使用本模块：
//! 1. 读取 MIDI 文件字节（一次性，解析后释放）
//! 2. 使用 MmapSmf 零拷贝 + 多轨道并行解析音符，写入硬盘缓存
//! 3. 构建帧索引，实现 O(log N) 视口音符窗口查询（按 start_tick 二分）
//! 4. 渲染每帧时从硬盘 seek+read 读取窗口内音符，渲染后立即丢弃

use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bytemuck;
use lumino_gfx::{NoteInstance, RenderParams};
use lumino_midi_loader::TICK_SEARCH_BUFFER;
use midly::mmap::MmapSmf;
use midly::{MetaMessage, MidiMessage, TrackEventKind};
use rayon::prelude::*;
use rustc_hash::FxHashMap;

use super::render_params::{SortableNote, build_note_rectangle_params_from_visible};
use super::{seconds_to_tick, ticks_to_seconds};

// ═══════════════════════════════════════════════════════════════════════════════
// 数据记录
// ═══════════════════════════════════════════════════════════════════════════════

/// 缓存中的单条音符记录（16 bytes）
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct NoteRecord {
    pub start_tick: u32,
    pub end_tick: u32,
    pub key: u16,
    pub velocity: u16,
    pub track: u16,
    pub channel: u16,
}

const NOTE_RECORD_SIZE: usize = std::mem::size_of::<NoteRecord>();

impl NoteRecord {
    pub fn length(&self) -> u32 {
        self.end_tick.saturating_sub(self.start_tick)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 帧索引
// ═══════════════════════════════════════════════════════════════════════════════

/// 帧索引条目：记录本帧需要渲染的音符范围
#[derive(Debug, Clone, Copy)]
pub struct FrameIndexEntry {
    /// 在 note_records 数组中的起始索引
    pub note_offset: u32,
    /// 本帧需要渲染的音符数量
    pub note_count: u32,
}

// ═══════════════════════════════════════════════════════════════════════════════
// 公共结果类型
// ═══════════════════════════════════════════════════════════════════════════════

/// 流式解析结果
pub struct StreamingMidiResult {
    /// 音符缓存文件路径
    pub note_cache_path: PathBuf,
    /// 帧索引
    pub frame_index: Vec<FrameIndexEntry>,
    /// PPQN
    pub ppqn: u32,
    /// 总 tick 数
    pub total_ticks: u32,
    /// 速度变化列表
    pub tempo_changes: Vec<(u32, f32)>,
    /// 拍号变化列表 (tick, 分子, 分母)
    pub time_signatures: Vec<(u32, u8, u8)>,
    /// 总帧数
    pub total_frames: u64,
}

/// 进度回调类型
pub type ProgressCallback = Arc<dyn Fn(String, f64) + Send + Sync>;

// ═══════════════════════════════════════════════════════════════════════════════
// 阶段 1：解析 MIDI → 写入硬盘缓存
// ═══════════════════════════════════════════════════════════════════════════════

/// 解析 MIDI 文件并构建硬盘缓存 + 帧索引
///
/// 使用 MmapSmf 零拷贝解析 + 多轨道并行处理。
/// `progress` 回调在解析阶段被调用：消息 + 0.0~1.0 进度。
pub fn parse_midi_to_cache(
    midi_path: &Path,
    fps: f64,
    viewport_beats: f64,
    progress: Option<ProgressCallback>,
) -> Result<StreamingMidiResult, String> {
    let note_cache_path = build_cache_path(midi_path)?;

    // ═══════════════════════════════════════════════════════════════════════
    // 阶段 1：读取 MIDI 文件 + 并行解析轨道（midi_bytes 在此作用域结束后释放）
    // ═══════════════════════════════════════════════════════════════════════
    let _ = send_progress(&progress, "正在扫描 MIDI 头部信息...".to_string(), 0.02);

    let (ppqn, scan_results) = {
        let midi_bytes =
            std::fs::read(midi_path).map_err(|e| format!("读取 MIDI 文件失败: {e}"))?;

        let mmap_smf = MmapSmf::parse(&midi_bytes).map_err(|e| format!("MmapSmf 解析失败: {e}"))?;

        let ppqn = match mmap_smf.header().timing {
            midly::Timing::Metrical(t) => u16::from(t) as u32,
            midly::Timing::Timecode(_, _) => 480,
        };

        let _ = send_progress(&progress, "正在并行解析所有轨道...".to_string(), 0.05);

        type ScanResult = (Vec<(u32, f32)>, Vec<(u32, u8, u8)>, u64, Vec<NoteRecord>);
        let scan_results: Vec<ScanResult> = mmap_smf
            .tracks()
            .par_iter()
            .enumerate()
            .map(|(track_idx, track)| {
                let mut local_tick: u64 = 0;
                let mut local_tempos: Vec<(u32, f32)> = Vec::new();
                let mut local_time_signatures: Vec<(u32, u8, u8)> = Vec::new();
                let mut max_tick: u64 = 0;
                let mut pending: FxHashMap<(u16, u16), PendingNote> = FxHashMap::default();
                let mut records: Vec<NoteRecord> = Vec::new();

                for ev in track.iter().flatten() {
                    local_tick += u32::from(ev.delta) as u64;
                    max_tick = max_tick.max(local_tick);

                    match ev.kind {
                        TrackEventKind::Meta(MetaMessage::Tempo(tempo)) => {
                            let bpm = 60_000_000.0 / tempo.as_int() as f32;
                            if bpm > 0.0 {
                                local_tempos.push((local_tick as u32, bpm));
                            }
                        }
                        TrackEventKind::Meta(MetaMessage::TimeSignature(num, den_power, _, _)) => {
                            let denominator = 1u8 << den_power;
                            local_time_signatures.push((local_tick as u32, num, denominator));
                        }
                        TrackEventKind::Midi { channel, message } => match message {
                            MidiMessage::NoteOn { key, vel } => {
                                let ch: u8 = channel.into();
                                let k: u16 = key.into();
                                let v: u8 = vel.into();
                                let pk = (ch as u16, k);

                                if let Some(pn) = pending.remove(&pk) {
                                    records.push(NoteRecord {
                                        start_tick: pn.start_tick,
                                        end_tick: local_tick as u32,
                                        key: k,
                                        velocity: pn.velocity,
                                        track: track_idx as u16,
                                        channel: ch as u16,
                                    });
                                }
                                if v > 0 {
                                    pending.insert(
                                        pk,
                                        PendingNote {
                                            start_tick: local_tick as u32,
                                            velocity: v as u16,
                                        },
                                    );
                                }
                            }
                            MidiMessage::NoteOff { key, .. } => {
                                let ch: u8 = channel.into();
                                let k: u16 = key.into();
                                let pk = (ch as u16, k);

                                if let Some(pn) = pending.remove(&pk) {
                                    records.push(NoteRecord {
                                        start_tick: pn.start_tick,
                                        end_tick: local_tick as u32,
                                        key: k,
                                        velocity: pn.velocity,
                                        track: track_idx as u16,
                                        channel: ch as u16,
                                    });
                                }
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                }

                for (_, pn) in pending.drain() {
                    records.push(NoteRecord {
                        start_tick: pn.start_tick,
                        end_tick: local_tick as u32,
                        key: 0,
                        velocity: pn.velocity,
                        track: track_idx as u16,
                        channel: 0,
                    });
                }

                (local_tempos, local_time_signatures, max_tick, records)
            })
            .collect();

        (ppqn, scan_results)
    }; // midi_bytes + mmap_smf 在此释放，释放约 1GB（对大 MIDI 文件）

    // ═══════════════════════════════════════════════════════════════════════
    // 阶段 2：合并轨道数据 → 排序 → 写入硬盘缓存
    // ═══════════════════════════════════════════════════════════════════════
    let _ = send_progress(&progress, "正在合并轨道数据...".to_string(), 0.65);

    let mut tempo_changes: Vec<(u32, f32)> = Vec::new();
    let mut time_signatures: Vec<(u32, u8, u8)> = Vec::new();
    let mut total_ticks: u64 = 0;
    let mut all_records: Vec<NoteRecord> = Vec::new();

    for (local_tempos, local_time_signatures, max_tick, local_records) in scan_results {
        tempo_changes.extend(local_tempos);
        time_signatures.extend(local_time_signatures);
        total_ticks = total_ticks.max(max_tick);
        all_records.extend(local_records);
    }

    if !tempo_changes.iter().any(|(t, _)| *t == 0) {
        tempo_changes.push((0, 120.0));
    }
    tempo_changes.sort_by_key(|a| a.0);
    tempo_changes.dedup_by(|a, b| {
        if a.0 == b.0 {
            std::mem::swap(a, b);
            true
        } else {
            false
        }
    });

    if !time_signatures.iter().any(|(t, _, _)| *t == 0) {
        time_signatures.push((0, 4, 4));
    }
    time_signatures.sort_by_key(|a| a.0);
    time_signatures.dedup_by(|a, b| {
        if a.0 == b.0 {
            std::mem::swap(a, b);
            true
        } else {
            false
        }
    });

    // 并行排序：按 start_tick 排序后写入缓存。
    // 帧索引基于 start_tick 二分窗口查询，保证每帧只读取视口窗口内的音符
    // （而非"视口起点到文件末尾"的全部音符）。
    all_records.par_sort_unstable_by_key(|r| r.start_tick);

    // 写入缓存文件
    {
        let note_file = std::fs::File::create(&note_cache_path)
            .map_err(|e| format!("创建音符缓存文件失败: {e}"))?;
        let mut note_writer = BufWriter::with_capacity(64 * 1024, note_file);

        note_writer
            .write_all(&(all_records.len() as u64).to_le_bytes())
            .map_err(|e| format!("写入缓存 header 失败: {e}"))?;

        for record in &all_records {
            let bytes: [u8; NOTE_RECORD_SIZE] = bytemuck::cast(*record);
            note_writer
                .write_all(&bytes)
                .map_err(|e| format!("写入音符记录失败: {e}"))?;
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 阶段 3：直接在内存中构建帧索引（无需重读缓存文件）
    // ═══════════════════════════════════════════════════════════════════════
    let _ = send_progress(&progress, "正在构建帧索引...".to_string(), 0.75);

    let frame_index = build_frame_index(
        &all_records,
        ppqn,
        total_ticks as u32,
        &tempo_changes,
        fps,
        viewport_beats,
    )?;

    let total_frames = frame_index.len() as u64;

    // all_records 在此函数末尾自动释放

    let _ = send_progress(&progress, "MIDI 缓存准备完成".to_string(), 1.0);

    Ok(StreamingMidiResult {
        note_cache_path,
        frame_index,
        ppqn,
        total_ticks: total_ticks as u32,
        tempo_changes,
        time_signatures,
        total_frames,
    })
}

#[derive(Debug, Clone, Copy)]
struct PendingNote {
    start_tick: u32,
    velocity: u16,
}

fn build_cache_path(midi_path: &Path) -> Result<PathBuf, String> {
    let stem = midi_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("lumino_video_export");
    let pid = std::process::id();
    let cache_name = format!("{stem}_video_export_notes_{pid}.bin");
    let mut path = std::env::temp_dir().join(cache_name);
    // 如果存在同名文件，追加计数器
    let mut counter = 1;
    while path.exists() {
        path = std::env::temp_dir().join(format!("{stem}_video_export_notes_{pid}_{counter}.bin"));
        counter += 1;
    }
    Ok(path)
}

fn send_progress(
    progress: &Option<ProgressCallback>,
    message: String,
    value: f64,
) -> Result<(), String> {
    if let Some(cb) = progress {
        cb(message, value.clamp(0.0, 1.0));
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 帧索引构建
// ═══════════════════════════════════════════════════════════════════════════════

fn build_frame_index(
    records: &[NoteRecord],
    ppqn: u32,
    total_ticks: u32,
    tempos: &[(u32, f32)],
    fps: f64,
    viewport_beats: f64,
) -> Result<Vec<FrameIndexEntry>, String> {
    let total_secs = ticks_to_seconds(total_ticks as u64, ppqn, tempos);
    let total_frames = (total_secs * fps).ceil() as u64;
    // 视口 tick 跨度 = 拍数 × PPQN（与内存模式 viewport_tick_span = ppq * 16 一致）
    let viewport_tick_span = ((ppqn as f64 * viewport_beats.max(1.0)) as u32).max(1);

    let note_count = records.len();

    if note_count == 0 {
        return Ok(Vec::new());
    }

    // 预计算全文件最大音符长度，用作搜索窗口下界的动态缓冲区。
    // 固定 TICK_SEARCH_BUFFER=19200 会导致时长超过该值的超长音符
    // 在 start_tick 远早于 vp_start 时被窗口排除，音符半路消失。
    let max_note_length = records
        .iter()
        .map(|r| r.end_tick.saturating_sub(r.start_tick))
        .max()
        .unwrap_or(0)
        .max(TICK_SEARCH_BUFFER);

    let index: Vec<FrameIndexEntry> = (0..total_frames)
        .into_par_iter()
        .map(|frame_idx| {
            let frame_time = frame_idx as f64 / fps;
            let center_tick = seconds_to_tick(frame_time, tempos, ppqn);
            let vp_start = center_tick;
            let vp_end = vp_start.saturating_add(viewport_tick_span);

            // 二分窗口 [left, right)：left = 第一个 start_tick >= vp_start - max_note_length
            // 的记录，right = 第一个 start_tick > vp_end 的记录。
            //
            // 正确性：视口内可见音符（end_tick >= vp_start && start_tick <= vp_end）必然
            // start_tick <= vp_end（在上界内）；任意时长的跨视口长音符必然满足
            // start_tick >= vp_start - max_note_length（因为 end_tick - start_tick <= max_note_length，
            // 而 end_tick >= vp_start → start_tick >= vp_start - max_note_length）。
            // 因此窗口是可见集合的超集，渲染前再按完整区间条件过滤即可。
            //
            // 性能：每帧读取量从"视口起点到文件末尾"（O(N)）降为 O(窗口内音符数)，
            // 修复导出耗时随总音符数线性上升的问题。
            let left = records
                .partition_point(|r| r.start_tick < vp_start.saturating_sub(max_note_length));
            let right = records.partition_point(|r| r.start_tick <= vp_end);

            FrameIndexEntry {
                note_offset: left as u32,
                note_count: right.saturating_sub(left) as u32,
            }
        })
        .collect();

    Ok(index)
}

// ═══════════════════════════════════════════════════════════════════════════════
// 时间转换（与 demo / video_export.rs 保持一致）
// ═══════════════════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════════════════
// 阶段 2：按帧流式读取并构建 RenderParams
// ═══════════════════════════════════════════════════════════════════════════════

/// 流式音符数据源
pub struct StreamingNoteSource {
    note_file: std::fs::File,
    note_cache_path: PathBuf,
    frame_index: Vec<FrameIndexEntry>,
    total_frames: u64,
    ppqn: u32,
    total_ticks: u32,
    tempo_changes: Vec<(u32, f32)>,
    /// 拍号变化列表 (tick, 分子, 分母)
    time_signatures: Vec<(u32, u8, u8)>,
    read_buf: Vec<u8>,
}

impl StreamingNoteSource {
    pub fn open(result: StreamingMidiResult) -> Result<Self, String> {
        let note_file = std::fs::File::open(&result.note_cache_path)
            .map_err(|e| format!("打开音符缓存文件失败: {e}"))?;
        Ok(Self {
            note_file,
            note_cache_path: result.note_cache_path,
            frame_index: result.frame_index,
            total_frames: result.total_frames,
            ppqn: result.ppqn,
            total_ticks: result.total_ticks,
            tempo_changes: result.tempo_changes,
            time_signatures: result.time_signatures,
            read_buf: Vec::new(),
        })
    }

    /// 返回缓存文件路径，用于导出完成后清理
    pub fn cache_path(&self) -> &Path {
        &self.note_cache_path
    }

    pub fn total_frames(&self) -> u64 {
        self.total_frames
    }

    pub fn total_ticks(&self) -> u32 {
        self.total_ticks
    }

    pub fn ppqn(&self) -> u32 {
        self.ppqn
    }

    pub fn tempo_changes(&self) -> &[(u32, f32)] {
        &self.tempo_changes
    }

    pub fn compute_duration_secs(&self) -> f64 {
        ticks_to_seconds(self.total_ticks as u64, self.ppqn, &self.tempo_changes)
    }

    /// 读取指定帧的可见音符并构建 RenderParams
    ///
    /// 使用已打开的 file seek + read_exact，避免每帧全量加载。
    /// 同时返回读取到的音符记录，供调用方计算按键高亮颜色。
    pub fn read_notes_and_params_for_frame(
        &mut self,
        frame_idx: u64,
        width: u32,
        height: u32,
        fps: f64,
    ) -> Result<(Vec<NoteRecord>, RenderParams), String> {
        if frame_idx >= self.total_frames {
            return Err("帧索引越界".to_string());
        }

        let entry = self.frame_index[frame_idx as usize];
        let mut notes: Vec<NoteRecord> = Vec::with_capacity(entry.note_count as usize);

        if entry.note_count > 0 {
            let offset = 8 + (entry.note_offset as u64) * (NOTE_RECORD_SIZE as u64);
            self.note_file
                .seek(SeekFrom::Start(offset))
                .map_err(|e| format!("seek 音符缓存失败: {e}"))?;

            let expected_bytes = (entry.note_count as usize) * NOTE_RECORD_SIZE;
            self.read_buf.resize(expected_bytes, 0);
            if let Err(e) = self.note_file.read_exact(&mut self.read_buf) {
                let file_len = std::fs::metadata(&self.note_cache_path)
                    .map(|m| m.len())
                    .unwrap_or(0);
                tracing::error!(
                    "read_exact 失败: frame_idx={}, note_offset={}, note_count={}, \
                     offset={}, expected_bytes={}, file_len={}, NOTE_RECORD_SIZE={}",
                    frame_idx,
                    entry.note_offset,
                    entry.note_count,
                    offset,
                    expected_bytes,
                    file_len,
                    NOTE_RECORD_SIZE,
                );
                return Err(format!("读取音符缓存失败: {e}"));
            }

            // SAFETY:
            // - `read_buf` 刚经 `read_exact` 读满 `expected_bytes = note_count * NOTE_RECORD_SIZE`，
            //   长度恰好容纳 `note_count` 个 `NoteRecord`，`from_raw_parts` 不越界。
            // - `NoteRecord` 为 `#[repr(C)]` + `bytemuck::Pod`（全 u32/u16 字段），无 padding、
            //   无 Drop、无非法值表示，字节序列合法。
            // - 对齐：`read_buf` 由 `Vec<u8>` 经全局分配器分配，返回指针满足 max_align_t
            //   （≥ align_of::<NoteRecord>() = 4）；下方 debug_assert 在 debug 构建兜底验证。
            unsafe {
                let ptr = self.read_buf.as_ptr() as *const NoteRecord;
                debug_assert_eq!(
                    ptr as usize % std::mem::align_of::<NoteRecord>(),
                    0,
                    "read_buf 未满足 NoteRecord 对齐要求"
                );
                notes.extend_from_slice(std::slice::from_raw_parts(ptr, entry.note_count as usize));
            }
        }

        let time_sec = frame_idx as f64 / fps;
        let tick = seconds_to_tick(time_sec, &self.tempo_changes, self.ppqn);
        let params = build_video_render_params_from_notes(
            width,
            height,
            tick,
            &notes,
            self.ppqn,
            &self.time_signatures,
        );

        Ok((notes, params))
    }
}

/// 从内存中的 NoteRecord 列表构建 RenderParams
///
/// 与内存模式共享排序分桶 + NoteInstance 构建（见 `render_params::build_note_rectangle_params_from_visible`），
/// 此处仅负责线性过滤可见音符并保留流式首帧诊断日志。
fn build_video_render_params_from_notes(
    width: u32,
    height: u32,
    tick: u32,
    notes: &[NoteRecord],
    ppq: u32,
    time_signatures: &[(u32, u8, u8)],
) -> RenderParams {
    let viewport_tick_span = (ppq * 16).max(1) as f32;
    let tick_start = tick;
    let tick_end = tick.saturating_add(viewport_tick_span as u32);

    // 线性过滤收集可见音符（流式缓存记录无需二分：单帧窗口内记录数有限）
    let mut visible: Vec<SortableNote> = Vec::with_capacity(notes.len());
    for n in notes.iter() {
        if n.end_tick >= tick_start && n.start_tick <= tick_end {
            visible.push(SortableNote {
                key: n.key as u8,
                start_tick: n.start_tick,
                length: n.length(),
                track_idx: n.track,
            });
        }
    }
    let mut note_instances: Vec<NoteInstance> = Vec::new();
    let params = build_note_rectangle_params_from_visible(
        width,
        height,
        tick,
        &mut visible,
        &mut note_instances,
        ppq,
        time_signatures,
    );

    // 首帧诊断：定位音符缺失问题
    static STREAM_DIAG_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let diag_idx = STREAM_DIAG_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if diag_idx < 3 {
        tracing::info!(
            "流式模式诊断[{}]: note_instances={}, notes_in_slice={}, tick={}, vis_range={}..{}",
            diag_idx,
            note_instances.len(),
            notes.len(),
            tick,
            tick_start,
            tick_end,
        );
    }
    params
}

#[cfg(test)]
mod tests {
    use super::*;

    const PPQN: u32 = 480;
    const FPS: f64 = 10.0;
    const TEMPOS: [(u32, f32); 1] = [(0, 120.0)];

    fn make_records(n: u32, total_ticks: u32) -> Vec<NoteRecord> {
        let mut records: Vec<NoteRecord> = (0..n)
            .map(|i| {
                // 均匀分布：间隔约 total_ticks / n，音符时长 240 ticks
                let t = i * (total_ticks / n).max(1);
                NoteRecord {
                    start_tick: t,
                    end_tick: t.saturating_add(240),
                    key: 60,
                    velocity: 100,
                    track: 0,
                    channel: 0,
                }
            })
            .collect();
        // 跨视口长音符（时长 = 2400 < TICK_SEARCH_BUFFER，必须被窗口覆盖）
        records.push(NoteRecord {
            start_tick: 1_000,
            end_tick: 3_400,
            key: 61,
            velocity: 100,
            track: 0,
            channel: 0,
        });
        records.sort_unstable_by_key(|r| r.start_tick);
        records
    }

    /// 正确性：每个帧的窗口必须覆盖该帧视口内所有**时长不超过
    /// `TICK_SEARCH_BUFFER`** 的可见音符（超集性质）。
    ///
    /// 可见判定与 `build_video_render_params_from_notes` 的过滤条件一致：
    /// `end_tick >= vp_start && start_tick <= vp_end`。
    ///
    /// 注意：时长超过 `TICK_SEARCH_BUFFER` 的超长跨视口音符会被窗口下界跳过，
    /// 这是与内存模式 `MidiDocument::get_track_notes_in_range` 一致的既有取舍。
    #[test]
    fn test_frame_index_window_covers_all_visible_notes() {
        let total_ticks = 576_000u32; // 10 分钟 @120bpm
        let records = make_records(200, total_ticks);

        let index = build_frame_index(&records, PPQN, total_ticks, &TEMPOS, FPS, 16.0)
            .expect("build_frame_index 不应失败");
        assert!(!index.is_empty());

        let viewport_span = (PPQN as f64 * 16.0) as u32;
        for (frame_idx, entry) in index.iter().enumerate() {
            let frame_time = frame_idx as f64 / FPS;
            let vp_start = seconds_to_tick(frame_time, &TEMPOS, PPQN);
            let vp_end = vp_start.saturating_add(viewport_span);
            let range = entry.note_offset as usize..(entry.note_offset + entry.note_count) as usize;

            for (i, r) in records.iter().enumerate() {
                let is_visible = r.end_tick >= vp_start && r.start_tick <= vp_end;
                let within_buffer = r.end_tick.saturating_sub(r.start_tick) <= TICK_SEARCH_BUFFER;
                if is_visible && within_buffer {
                    assert!(
                        range.contains(&i),
                        "帧 {frame_idx} (vp {vp_start}..{vp_end}) 遗漏可见记录 {i}: \
                         start={} end={}",
                        r.start_tick,
                        r.end_tick,
                    );
                }
            }
        }
    }

    /// 性能护栏：大文件下每帧窗口大小必须远小于总记录数。
    ///
    /// 旧实现窗口 = [视口起点, 文件末尾)，帧 0 即读取全部记录（O(N) 每帧），
    /// 导出速度随总音符数线性下降。修复后窗口仅覆盖视口附近
    /// （±TICK_SEARCH_BUFFER 扩展），大小与总记录数无关。
    #[test]
    fn test_frame_index_window_stays_small_for_large_files() {
        let total_ticks = 576_000u32; // 10 分钟 @120bpm
        const RECORD_COUNT: usize = 10_000;
        let records = make_records(RECORD_COUNT as u32, total_ticks);

        let index = build_frame_index(&records, PPQN, total_ticks, &TEMPOS, FPS, 16.0)
            .expect("build_frame_index 不应失败");

        let max_window = index
            .iter()
            .map(|e| e.note_count as usize)
            .max()
            .expect("帧索引不应为空");
        // 窗口仅覆盖视口 ± 缓冲区（~34560 ticks / 576000 ticks ≈ 6% 的记录），
        // 远小于总数；旧实现首帧窗口 = RECORD_COUNT。
        assert!(
            max_window * 20 < RECORD_COUNT,
            "帧索引窗口过大: max={max_window}, 总记录={RECORD_COUNT}"
        );
    }
}
