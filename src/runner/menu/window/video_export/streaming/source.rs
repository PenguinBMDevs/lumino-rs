//! 流式音符数据源：按帧 seek+read 硬盘缓存并构建 RenderParams

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use lumino_gfx::{NoteInstance, RenderParams};

use super::super::render_params::{
    NoteRectangleParamsInput, SortableNote, build_note_rectangle_params_from_visible,
};
use super::super::{seconds_to_tick, ticks_to_seconds};
use super::{FrameIndexEntry, NOTE_RECORD_SIZE, NoteRecord, StreamingMidiResult};

/// 流式音符数据源
pub struct StreamingNoteSource {
    note_file: File,
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
        let note_file = File::open(&result.note_cache_path)
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
    let params = build_note_rectangle_params_from_visible(NoteRectangleParamsInput {
        width,
        height,
        tick,
        visible_notes: &mut visible,
        note_instances_out: &mut note_instances,
        ppq,
        time_signatures,
    });

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
