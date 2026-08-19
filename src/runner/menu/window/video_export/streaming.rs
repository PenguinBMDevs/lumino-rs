//! 视频导出流式 MIDI 数据源
//!
//! 当内存中没有 MidiDocument 时，使用本模块：
//! 1. 读取 MIDI 文件字节（一次性，解析后释放）
//! 2. 使用 MmapSmf 零拷贝 + 多轨道并行解析音符，写入硬盘缓存
//! 3. 构建帧索引，实现 O(log N) 视口音符窗口查询（按 start_tick 二分）
//! 4. 渲染每帧时从硬盘 seek+read 读取窗口内音符，渲染后立即丢弃

use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bytemuck;
use midly::mmap::MmapSmf;
use midly::{MetaMessage, MidiMessage, TrackEventKind};
use rayon::prelude::*;
use rustc_hash::FxHashMap;

mod cache;
mod frame_index;
mod source;

#[cfg(test)]
mod tests;

pub(crate) use cache::{build_cache_path, send_progress};
pub(crate) use frame_index::build_frame_index;
pub use frame_index::FrameIndexEntry;
pub use source::StreamingNoteSource;

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

#[derive(Debug, Clone, Copy)]
struct PendingNote {
    start_tick: u32,
    velocity: u16,
}

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
