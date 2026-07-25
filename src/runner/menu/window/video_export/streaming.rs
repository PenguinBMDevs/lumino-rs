//! 视频导出流式 MIDI 数据源
//!
//! 当内存中没有 MidiDocument 时，使用本模块：
//! 1. 读取 MIDI 文件字节（一次性，解析后释放）
//! 2. 使用 StreamingMidiPlayer 流式解析事件，将音符记录写入硬盘缓存
//! 3. 构建帧索引，实现 O(1) 视口音符范围查询
//! 4. 渲染每帧时从硬盘 seek+read 读取可见音符，渲染后立即丢弃

use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bytemuck;
use lumino_gfx::{
    ARRANGEMENT_PALETTE, NoteInstance, RenderParams, generate_ruler_instances, pack_color,
};
use midly::{MidiMessage, TrackEventKind};
use rayon::prelude::*;
use rustc_hash::FxHashMap;

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
/// `progress` 回调在解析阶段被调用：消息 + 0.0~1.0 进度。
pub fn parse_midi_to_cache(
    midi_path: &Path,
    fps: f64,
    viewport_beats: f64,
    progress: Option<ProgressCallback>,
) -> Result<StreamingMidiResult, String> {
    let midi_bytes = std::fs::read(midi_path).map_err(|e| format!("读取 MIDI 文件失败: {e}"))?;

    let _ = send_progress(&progress, "正在扫描 MIDI 头部信息...".to_string(), 0.02);

    let mut player = lumino_midi_loader::StreamingMidiPlayer::from_bytes(&midi_bytes)
        .map_err(|e| format!("StreamingMidiPlayer 初始化失败: {e}"))?;

    let ppqn = player.ppqn();
    let total_ticks = player.total_ticks() as u32;
    let tempo_changes: Vec<(u32, f32)> = player.tempo_changes().to_vec();

    let _ = send_progress(&progress, "正在解析音符并写入硬盘缓存...".to_string(), 0.10);

    // 创建临时缓存文件：放在输出目录附近，避免跨盘 IO
    let note_cache_path = build_cache_path(midi_path)?;
    let note_file = std::fs::File::create(&note_cache_path)
        .map_err(|e| format!("创建音符缓存文件失败: {e}"))?;
    let mut note_writer = BufWriter::with_capacity(64 * 1024, note_file);

    // 预留 header：note_count (8 bytes)
    note_writer
        .write_all(&[0u8; 8])
        .map_err(|e| format!("写入缓存 header 失败: {e}"))?;

    let mut pending: FxHashMap<(u16, u16, u16), PendingNote> = FxHashMap::default();
    let mut note_count: u64 = 0;
    let mut last_reported_percent: u32 = 0;
    let mut event_counter: u64 = 0;

    while let Some((tick_u64, track_idx, kind)) = player.next_event() {
        let tick = tick_u64 as u32;
        event_counter += 1;

        if event_counter & 0x3F == 0 {
            let percent = if total_ticks > 0 {
                (tick as u64 * 100 / player.total_ticks().max(1)) as u32
            } else {
                0
            };
            if percent > last_reported_percent && percent <= 100 {
                let _ = send_progress(
                    &progress,
                    "正在解析音符并写入硬盘缓存...".to_string(),
                    0.10 + (percent as f64) * 0.60 / 100.0,
                );
                last_reported_percent = percent;
            }
        }

        match kind {
            TrackEventKind::Midi {
                channel,
                message: MidiMessage::NoteOn { key, vel },
            } => {
                let ch: u8 = channel.into();
                let k: u16 = key.into();
                let v: u8 = vel.into();
                let pending_key = (track_idx as u16, ch as u16, k);

                if let Some(pn) = pending.remove(&pending_key) {
                    emit_note_record(&mut note_writer, tick, pn, track_idx as u16, ch as u16, k)?;
                    note_count += 1;
                }
                if v > 0 {
                    pending.insert(
                        pending_key,
                        PendingNote {
                            start_tick: tick,
                            velocity: v as u16,
                        },
                    );
                }
            }
            TrackEventKind::Midi {
                channel,
                message: MidiMessage::NoteOff { key, vel: _ },
            } => {
                let ch: u8 = channel.into();
                let k: u16 = key.into();
                let pending_key = (track_idx as u16, ch as u16, k);

                if let Some(pn) = pending.remove(&pending_key) {
                    emit_note_record(&mut note_writer, tick, pn, track_idx as u16, ch as u16, k)?;
                    note_count += 1;
                }
            }
            _ => {}
        }
    }

    // 处理孤立的 NoteOn
    for (_, pn) in pending.drain() {
        emit_note_record(&mut note_writer, total_ticks, pn, 0, 0, 0)?;
        note_count += 1;
    }

    // 回写 note_count header
    // 使用 BufWriter::seek 而非 get_mut().seek()，因为 BufWriter::seek 会先 flush
    // 内部缓冲再 seek，防止剩余缓冲数据在 drop 时冲刷到错误位置（position 8），
    // 导致大文件时缓存文件比预期小，read_exact 读不到足够数据。
    note_writer
        .seek(SeekFrom::Start(0))
        .map_err(|e| format!("seek 到缓存 header 失败: {e}"))?;
    note_writer
        .write_all(&note_count.to_le_bytes())
        .map_err(|e| format!("写入缓存 note_count 失败: {e}"))?;
    drop(note_writer);

    let _ = send_progress(&progress, "正在构建帧索引...".to_string(), 0.75);

    let frame_index = build_frame_index(
        &note_cache_path,
        note_count,
        ppqn,
        total_ticks,
        &tempo_changes,
        fps,
        viewport_beats,
    )?;

    let total_frames = frame_index.len() as u64;

    let _ = send_progress(&progress, "MIDI 缓存准备完成".to_string(), 1.0);

    // 释放 MIDI 字节内存
    drop(midi_bytes);

    Ok(StreamingMidiResult {
        note_cache_path,
        frame_index,
        ppqn,
        total_ticks,
        tempo_changes,
        total_frames,
    })
}

#[derive(Debug, Clone, Copy)]
struct PendingNote {
    start_tick: u32,
    velocity: u16,
}

fn emit_note_record(
    writer: &mut BufWriter<std::fs::File>,
    end_tick: u32,
    pn: PendingNote,
    track: u16,
    channel: u16,
    key: u16,
) -> Result<(), String> {
    let record = NoteRecord {
        start_tick: pn.start_tick,
        end_tick,
        key,
        velocity: pn.velocity,
        track,
        channel,
    };
    let bytes: [u8; NOTE_RECORD_SIZE] = bytemuck::cast(record);
    writer
        .write_all(&bytes)
        .map_err(|e| format!("写入音符记录失败: {e}"))?;
    Ok(())
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
    note_cache_path: &Path,
    note_count: u64,
    ppqn: u32,
    total_ticks: u32,
    tempos: &[(u32, f32)],
    fps: f64,
    viewport_beats: f64,
) -> Result<Vec<FrameIndexEntry>, String> {
    let total_secs = ticks_to_seconds(total_ticks as u64, ppqn, tempos);
    let total_frames = (total_secs * fps).ceil() as u64;
    let _ = viewport_beats;

    if note_count == 0 {
        return Ok(Vec::new());
    }

    let mut note_file =
        std::fs::File::open(note_cache_path).map_err(|e| format!("打开音符缓存文件失败: {e}"))?;

    let mut header = [0u8; 8];
    note_file
        .read_exact(&mut header)
        .map_err(|e| format!("读取缓存 header 失败: {e}"))?;
    let cached_note_count = u64::from_le_bytes(header);
    if cached_note_count != note_count {
        return Err(format!(
            "缓存音符数量不匹配: header={}, 实际={}",
            cached_note_count, note_count
        ));
    }

    let mut note_data = Vec::with_capacity((note_count as usize) * NOTE_RECORD_SIZE);
    note_file
        .read_to_end(&mut note_data)
        .map_err(|e| format!("读取音符缓存数据失败: {e}"))?;

    let expected_bytes = (note_count as usize) * NOTE_RECORD_SIZE;
    if note_data.len() < expected_bytes {
        return Err(format!(
            "缓存音符数据不完整: 预期 {} 个记录 ({} bytes), 实际读取 {} bytes",
            note_count,
            expected_bytes,
            note_data.len(),
        ));
    }

    let records: &[NoteRecord] = unsafe {
        std::slice::from_raw_parts(note_data.as_ptr() as *const NoteRecord, note_count as usize)
    };

    let index: Vec<FrameIndexEntry> = (0..total_frames)
        .into_par_iter()
        .map(|frame_idx| {
            let frame_time = frame_idx as f64 / fps;
            let center_tick = seconds_to_tick(frame_time, ppqn, tempos);
            let vp_start = center_tick;

            let left = records.partition_point(|r| r.end_tick < vp_start);

            FrameIndexEntry {
                note_offset: left as u32,
                note_count: (note_count as usize - left) as u32,
            }
        })
        .collect();

    Ok(index)
}

// ═══════════════════════════════════════════════════════════════════════════════
// 时间转换（与 demo / video_export.rs 保持一致）
// ═══════════════════════════════════════════════════════════════════════════════

fn ticks_to_seconds(tick: u64, ppqn: u32, tempos: &[(u32, f32)]) -> f64 {
    let mut total_secs = 0.0_f64;
    let mut prev_tick: u32 = 0;
    let mut prev_bpm: f32 = 120.0;

    for &(t, bpm) in tempos {
        let segment_ticks = (t.saturating_sub(prev_tick)) as u64;
        let segment_secs = segment_ticks as f64 * 60.0 / (prev_bpm as f64 * ppqn as f64);
        total_secs += segment_secs;

        if tick <= t as u64 {
            let within_ticks = tick.saturating_sub(prev_tick as u64);
            let within_secs = within_ticks as f64 * 60.0 / (prev_bpm as f64 * ppqn as f64);
            return total_secs - segment_secs + within_secs;
        }

        prev_tick = t;
        prev_bpm = bpm;
    }

    let remaining = tick.saturating_sub(prev_tick as u64);
    total_secs + remaining as f64 * 60.0 / (prev_bpm as f64 * ppqn as f64)
}

fn seconds_to_tick(secs: f64, ppqn: u32, tempos: &[(u32, f32)]) -> u32 {
    let mut accum_secs = 0.0_f64;
    let mut prev_tick: u32 = 0;
    let mut prev_bpm: f32 = 120.0;

    for &(t, bpm) in tempos {
        let segment_ticks = (t.saturating_sub(prev_tick)) as u64;
        let segment_secs = segment_ticks as f64 * 60.0 / (prev_bpm as f64 * ppqn as f64);

        if accum_secs + segment_secs >= secs {
            let within_secs = secs - accum_secs;
            let within_ticks = (within_secs * prev_bpm as f64 * ppqn as f64 / 60.0) as u32;
            return prev_tick + within_ticks;
        }

        accum_secs += segment_secs;
        prev_tick = t;
        prev_bpm = bpm;
    }

    let remaining_secs = secs - accum_secs;
    let remaining_ticks = (remaining_secs * prev_bpm as f64 * ppqn as f64 / 60.0) as u32;
    prev_tick + remaining_ticks
}

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

            unsafe {
                let ptr = self.read_buf.as_ptr() as *const NoteRecord;
                notes.extend_from_slice(std::slice::from_raw_parts(ptr, entry.note_count as usize));
            }
        }

        let time_sec = frame_idx as f64 / fps;
        let tick = seconds_to_tick(time_sec, self.ppqn, &self.tempo_changes);
        let params = build_video_render_params_from_notes(width, height, tick, &notes, self.ppqn);

        Ok((notes, params))
    }
}

/// 从内存中的 NoteRecord 列表构建 RenderParams
///
/// 与 video_export.rs 中 `build_video_render_params` 行为一致，但数据源是流式读取的音符。
fn build_video_render_params_from_notes(
    width: u32,
    height: u32,
    tick: u32,
    notes: &[NoteRecord],
    ppq: u32,
) -> RenderParams {
    const KEY_COUNT: u16 = 128;

    let keyboard_width = 60.0f32;
    let ruler_height = 30.0f32;
    let w = width.max(1) as f32;
    let h = height.max(1) as f32;

    let viewport_tick_span = (ppq * 16).max(1) as f32;
    let zoom_x = (w - keyboard_width) / viewport_tick_span;
    let key_count_f = KEY_COUNT as f32;
    let zoom_y = (h - ruler_height) / key_count_f;

    let scroll_x = tick as f32 * zoom_x;
    let scroll_y = 0.0f32;

    let grid_instances = Vec::new();
    let ruler_instances =
        generate_ruler_instances(w, keyboard_width, ruler_height, scroll_x, zoom_x);
    let keyboard_instances = Vec::new();

    let tick_start = tick;
    let tick_end = tick.saturating_add(viewport_tick_span as u32);

    let mut temp: Vec<SortableNote> = Vec::with_capacity(notes.len());
    for n in notes.iter() {
        if n.end_tick >= tick_start && n.start_tick <= tick_end {
            temp.push(SortableNote {
                key: n.key as u8,
                start_tick: n.start_tick,
                length: n.length(),
                track_idx: n.track,
            });
        }
    }
    temp.sort_unstable_by_key(|n| (n.key, n.start_tick, u16::MAX - n.track_idx));

    let note_instances: Vec<NoteInstance> = temp
        .into_iter()
        .map(|n| {
            let color = ARRANGEMENT_PALETTE[n.track_idx as usize % ARRANGEMENT_PALETTE.len()];
            let color_packed = pack_color([color[0], color[1], color[2], 1.0]);
            NoteInstance {
                position: [n.start_tick as f32, n.key as f32],
                size_x: (n.length as f32).max(1.0),
                color_packed,
            }
        })
        .collect();

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

    let max_key_index = (KEY_COUNT.saturating_sub(1)) as f32;
    let canvas_size = (w, h);

    RenderParams {
        viewport_size: (width.max(1), height.max(1)),
        logical_size: (w, h),
        scale_factor: 1.0,
        scroll: (scroll_x, scroll_y),
        zoom: (zoom_x, zoom_y),
        keyboard_width,
        ruler_height,
        note_instances,
        grid_instances,
        ruler_instances,
        keyboard_instances,
        ppq: ppq as f32,
        max_key_index,
        canvas_size,
        ..Default::default()
    }
}

#[derive(Clone)]
struct SortableNote {
    key: u8,
    start_tick: u32,
    length: u32,
    track_idx: u16,
}
