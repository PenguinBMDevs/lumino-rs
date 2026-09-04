//! MidiDocument 构造与加载（从 `document.rs` 拆分）
//!
//! 文档构建入口：空白文档构造、MIDI 文件/字节加载、每轨 max_end_tick 缓存初始化。

use std::path::Path;

use lumino_diagnostics::memory_monitor::MemoryMonitor;

use crate::error::{LoaderError, LoaderResult};
use crate::note_event::NoteEvent;
use crate::track::TrackManager;

use super::{MidiDocument, scan};

/// 按 tick 排序去重；若首项 tick != 0，在头部插入 tick=0 的默认事件。
///
/// 收敛 `from_notes_bytes` 中 tempo/time_sig/key_sig 三处同构的
/// "sort → dedup → 首项补默认" 模板。
fn finalize_sorted_events<T>(
    events: &mut Vec<T>,
    get_tick: impl Fn(&T) -> u32,
    default_at_zero: T,
) {
    events.sort_unstable_by_key(&get_tick);
    events.dedup_by(|a, b| get_tick(a) == get_tick(b));
    if events.first().is_none_or(|e| get_tick(e) != 0) {
        events.insert(0, default_at_zero);
    }
}

impl MidiDocument {
    /// 构造空白文档（N 条空轨道，供新建工程初始化为可编辑空白工程）。
    ///
    /// 2026-08 修复：新建工程/关闭文件后 `clear_editor` 复用本构造重建空文档，
    /// 使 `insert_note` 不再因 `document = None` 拦截音符创建（空白工程可直接编辑）。
    ///
    /// 轨道命名与 `Sidebar::new()` 默认轨道一致：轨道 0 = "Conductor"，轨道 1 = "Setup"，
    /// 其余为 "Track {i}"。`division` 为文档头 PPQ（调用方传入，通常是编辑器当前 PPQ，
    /// 避免新建工程落盘时回退到硬编码 480）。
    pub fn empty_with_tracks(track_count: u16, division: u16) -> Self {
        Self {
            notes: (0..track_count)
                .map(|_| crate::chunked_list::ChunkedList::new())
                .collect(),
            next_note_id: 1,
            tempo_changes: vec![(0, 120.0)],
            time_signatures: vec![(0, 4, 4)],
            key_signatures: Vec::new(),
            control_events: crate::chunked_list::ChunkedList::new(),
            lyrics: Vec::new(),
            markers: Vec::new(),
            sys_ex: Vec::new(),
            track_names: (0..track_count)
                .map(|i| match i {
                    0 => Some("Conductor".to_string()),
                    1 => Some("Setup".to_string()),
                    _ => Some(format!("Track {i}")),
                })
                .collect(),
            total_ticks: 0,
            track_count,
            tracks: crate::track::TrackManager::new(track_count),
            division,
            track_ports: vec![0; track_count as usize],
            track_max_end_ticks: Self::new_track_max_ticks(track_count as usize),
        }
    }

    /// 使用 midly 从 MIDI 文件加载并构建紧凑内存文档。
    pub fn from_notes_file<P: AsRef<Path>>(
        midi_path: P,
        progress: Option<&dyn Fn(f64)>,
    ) -> LoaderResult<Self> {
        let path = midi_path.as_ref();
        let file_bytes = std::fs::read(path).map_err(LoaderError::Io)?;
        let (doc, _, _) = Self::from_notes_bytes(&file_bytes, progress)?;
        Ok(doc)
    }

    /// 使用 midly 从 MIDI 字节加载并构建紧凑内存文档。
    pub fn from_notes_bytes(
        file_bytes: &[u8],
        progress: Option<&dyn Fn(f64)>,
    ) -> LoaderResult<(Self, u16, u64)> {
        MemoryMonitor::global().check();

        if let Some(cb) = progress {
            (cb)(0.05);
        }

        let (_, division) = scan::scan_header_info(file_bytes)
            .ok_or_else(|| LoaderError::FileFormat("无法解析 MIDI 文件头".to_string()))?;

        if let Some(cb) = progress {
            (cb)(0.10);
        }

        // 使用 MIDI 标签追踪解析过程中的密集内存分配
        lumino_diagnostics::memtrace::with_tag(lumino_diagnostics::memtrace::AllocTag::Midi, || {
            // 加载期全局 ID 分配器：跨轨单调递增，保证加载出的音符全局唯一
            // 用 AtomicU64（midly loader 回调要求 Send，Cell 不满足）
            let next_id = std::sync::atomic::AtomicU64::new(1u64);
            let track_names = scan::scan_track_names(file_bytes);

            if let Some(cb) = progress {
                (cb)(0.15);
            }

            let mut notes: Vec<crate::chunked_list::ChunkedList<NoteEvent>> = Vec::new();
            let mut track_ports: Vec<u8> = Vec::new();
            let mut all_tempo_changes: Vec<(u32, f32)> = Vec::new();
            let mut all_time_signatures: Vec<(u32, u8, u8)> = Vec::new();
            let mut all_key_signatures: Vec<(u32, i8, bool)> = Vec::new();
            let mut control_events: Vec<midly::loader::PackedControlEvent> = Vec::new();
            let mut lyrics: Vec<(u32, u16, Vec<u8>)> = Vec::new();
            let mut markers: Vec<(u32, u16, Vec<u8>)> = Vec::new();
            let mut sys_ex: Vec<(u32, u16, Vec<u8>)> = Vec::new();
            let mut total_notes: u64 = 0;
            let mut total_ticks: u32 = 0;

            midly::loader::extract_all_events_per_track_streaming_from_bytes(
                file_bytes,
                |track_idx, mut events| {
                    if track_idx >= notes.len() {
                        notes.resize_with(track_idx + 1, crate::chunked_list::ChunkedList::new);
                        track_ports.resize(track_idx + 1, 0);
                    }

                    // 2026-08-15 峰值优化：不再经中间 Vec<NoteEvent> 中转，
                    // PackedNote 直接分块转换进 ChunkedList（省 16B/音符的峰值内存，
                    // 2.9 亿音符场景 ≈ 4.6GB）。
                    if let Some(last) = events.notes.iter().max_by_key(|n| n.end_tick) {
                        total_ticks = total_ticks.max(last.end_tick);
                    }
                    total_notes += events.notes.len() as u64;

                    // midly 的 TrackAllEvents 按「NoteOff 到达顺序」产出音符（重叠音符中
                    // 先结束后开始的会排在先开始音符之前），并不保证按 start_tick 升序——
                    // 其文档注释「sorted by start tick」对该流式路径并不成立。
                    // ChunkedList 依赖 start_tick 升序做区间查询（range / 走带视图），
                    // 必须在转换前显式排序。仅对 midly 已物化的本轨 Vec<PackedNote>
                    // 做原地排序，不引入额外峰值内存（无第二个 Vec<NoteEvent> 常驻）。
                    events.notes.sort_by_key(|n| n.start_tick);
                    notes[track_idx] = crate::chunked_list::ChunkedList::from_sorted_iter(
                        events.notes.into_iter().map(|p| {
                            NoteEvent::from(p)
                                .with_id(next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
                        }),
                    );

                    // MidiPort meta (FF 21)：流式提取首个出现值（与旧 Smf::parse
                    // 语义一致），不再对文件做第二次全量解析——2.9 亿音符的
                    // 黑乐谱此前在此产生 15-18GB 临时峰值。
                    track_ports[track_idx] = events.midi_port.unwrap_or(0);

                    all_tempo_changes.extend(events.tempo_changes);
                    control_events.extend(events.control_events);
                    all_time_signatures.extend(events.time_signatures.into_iter().map(|ts| {
                        (
                            ts.tick,
                            ts.numerator,
                            1u32.wrapping_shl(ts.denominator as u32) as u8,
                        )
                    }));
                    all_key_signatures.extend(
                        events
                            .key_signatures
                            .into_iter()
                            .map(|ks| (ks.tick, ks.sharps, ks.is_minor)),
                    );
                    lyrics.extend(
                        events
                            .lyrics
                            .into_iter()
                            .map(|ev| (ev.tick, ev.track, ev.text.to_vec())),
                    );
                    markers.extend(
                        events
                            .markers
                            .into_iter()
                            .map(|ev| (ev.tick, ev.track, ev.text.to_vec())),
                    );
                    sys_ex.extend(
                        events
                            .sys_ex
                            .into_iter()
                            .map(|ev| (ev.tick, ev.track, ev.data.to_vec())),
                    );
                },
            )
            .map_err(|e| LoaderError::MidiParse(format!("提取音符失败: {e}")))?;

            finalize_sorted_events(&mut all_tempo_changes, |&(t, _)| t, (0u32, 120.0f32));
            finalize_sorted_events(&mut all_time_signatures, |&(t, _, _)| t, (0u32, 4u8, 4u8));
            finalize_sorted_events(&mut all_key_signatures, |&(t, _, _)| t, (0u32, 0i8, false));

            // 稳定排序保留同 tick 文件序：RPN(CC101/100/6/38) 必须在同 tick 的 PitchBend 之前，
            // 否则 PB 会用旧的 bend_sensitivity（默认 2 半音）而非 RPN 设定的值（如 24）。
            // 复刻 yinhe 2026-06-27 13:22 fix(audio): RPN 展开必须在 PB 之前。
            control_events.sort_by_key(|e| e.tick);
            lyrics.sort_by_key(|e| e.0);
            markers.sort_by_key(|e| e.0);
            sys_ex.sort_by_key(|e| e.0);

            if let Some(cb) = progress {
                (cb)(0.75);
            }

            let track_count = notes.len() as u16;
            let tracks_manager = TrackManager::new(track_count);

            if let Some(cb) = progress {
                (cb)(0.90);
            }

            Ok((
                Self {
                    notes,
                    next_note_id: next_id.load(std::sync::atomic::Ordering::Relaxed),
                    tempo_changes: all_tempo_changes,
                    time_signatures: all_time_signatures,
                    key_signatures: all_key_signatures,
                    control_events: crate::chunked_list::ChunkedList::from_sorted(control_events),
                    lyrics,
                    markers,
                    sys_ex,
                    track_names,
                    total_ticks,
                    track_count,
                    tracks: tracks_manager,
                    division,
                    track_ports,
                    // max_end_tick 缓存：初始 None（脏），首次查询惰性重算
                    track_max_end_ticks: Self::new_track_max_ticks(track_count as usize),
                },
                division,
                total_notes,
            ))
        })
    }

    /// 构造每轨 max_end_tick 缓存（N 个**独立** `Arc<Mutex>`）。
    ///
    /// 注意：不能用 `vec![Arc::new(Mutex::new(None)); N]` —— 该写法把同一个 Arc
    /// 克隆 N 次，导致所有音轨共享**同一个** Mutex（缓存串号）。必须逐个构造。
    pub fn new_track_max_ticks(n: usize) -> Vec<std::sync::Arc<std::sync::Mutex<Option<u32>>>> {
        (0..n)
            .map(|_| std::sync::Arc::new(std::sync::Mutex::new(None)))
            .collect()
    }
}
