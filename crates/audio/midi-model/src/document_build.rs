//! MidiDocument 构造与加载（从 `document.rs` 拆分）
//!
//! 文档构建入口：空白文档构造、MIDI 文件/字节加载、每轨 max_end_tick 缓存初始化。

use std::path::Path;

use lumino_core::{tr, trf};
use lumino_diagnostics::memory_monitor::MemoryMonitor;

use crate::error::{LoaderError, LoaderResult};
use crate::note_event::NoteEvent;
use crate::track::TrackManager;

use super::{MidiDocument, scan};

/// 按 tick 稳定排序；若首项 tick != 0，在头部插入 tick=0 的默认事件。
///
/// 收敛 `from_notes_bytes` 中 tempo/time_sig/key_sig 三处同构的
/// "sort → 首项补默认" 模板。
///
/// 加载 = 全量保留：同 tick 的多个 tempo/拍号/调号事件全部保留，
/// 不做去重（历史 `dedup_by` 会静默丢弃同 tick 事件，与 EXP-005 的
/// 逐事件往返等价验收冲突）。稳定排序保证同 tick 事件的文件序。
fn finalize_sorted_events<T>(
    events: &mut Vec<T>,
    get_tick: impl Fn(&T) -> u32,
    default_at_zero: T,
) {
    events.sort_by_key(&get_tick);
    if events.first().is_none_or(|e| get_tick(e) != 0) {
        events.insert(0, default_at_zero);
    }
}

/// 单轨构建产物（Tier2 逐轨并行：排序 / ChunkedList 构建 / 文本复制在锁外完成）。
///
/// 由 `extract_all_events_per_track_collect_from_bytes` 的闭包产出，
/// 按轨序收集后统一归并；持有它等价于持有该轨最终要保留的数据本身
/// （音符 ChunkedList + 各事件 Vec），不产生额外的全量副本。
struct TrackBuild {
    notes: crate::chunked_list::ChunkedList<NoteEvent>,
    port: u8,
    /// TrackName meta（首个，已解码；空名称归一为 None，与旧 `scan_track_names` 一致）。
    name: Option<String>,
    note_count: u64,
    tempo_changes: Vec<(u32, f32)>,
    control_events: Vec<midly::loader::PackedControlEvent>,
    time_signatures: Vec<(u32, u8, u8)>,
    key_signatures: Vec<(u32, i8, bool)>,
    lyrics: Vec<(u32, u16, Vec<u8>)>,
    markers: Vec<(u32, u16, Vec<u8>)>,
    text_events: Vec<(u32, u16, u8, Vec<u8>)>,
    sys_ex: Vec<(u32, u16, Vec<u8>)>,
    /// EOT / 悬挂音符的逐轨 timing（midly `TrackTiming`，直接透传）。
    timing: midly::loader::TrackTiming,
}

/// 逐轨产物按轨序归并后的累加器（只含最终落进文档的数据）。
struct MergedTracks {
    notes: Vec<crate::chunked_list::ChunkedList<NoteEvent>>,
    track_ports: Vec<u8>,
    track_names: Vec<Option<String>>,
    all_tempo_changes: Vec<(u32, f32)>,
    all_time_signatures: Vec<(u32, u8, u8)>,
    all_key_signatures: Vec<(u32, i8, bool)>,
    control_events: Vec<midly::loader::PackedControlEvent>,
    lyrics: Vec<(u32, u16, Vec<u8>)>,
    markers: Vec<(u32, u16, Vec<u8>)>,
    text_events: Vec<(u32, u16, u8, Vec<u8>)>,
    sys_ex: Vec<(u32, u16, Vec<u8>)>,
    total_notes: u64,
    total_ticks: u32,
    track_timings: Vec<midly::loader::TrackTiming>,
}

/// 把 midly 单轨事件物化为 `TrackBuild`（逐轨并行回调的函数体）。
fn track_build_from_events(mut events: midly::loader::TrackAllEvents<'_>) -> TrackBuild {
    let note_count = events.notes.len() as u64;
    // midly 的 TrackAllEvents 按「NoteOff 到达顺序」产出音符（重叠音符中
    // 先结束后开始的会排在先开始音符之前），并不保证按 start_tick 升序——
    // 其文档注释「sorted by start tick」对该流式路径并不成立。
    // ChunkedList 依赖 start_tick 升序做区间查询（range / 走带视图），
    // 必须在转换前显式排序。仅对 midly 已物化的本轨 Vec<PackedNote>
    // 做原地排序，不引入额外峰值内存（无第二个 Vec<NoteEvent> 常驻）。
    // 巨轨用稳定基数排序（8.8M 音符 330→80ms）；小轨/已排序轨走标准库。
    crate::note_sort::sort_by_start_tick(&mut events.notes);
    TrackBuild {
        notes: crate::chunked_list::ChunkedList::from_sorted_iter(
            events.notes.into_iter().map(NoteEvent::from),
        ),
        // MidiPort meta (FF 21)：首个出现值（与旧 Smf::parse 语义一致）。
        port: events.midi_port.unwrap_or(0),
        // TrackName meta (FF 03)：快路径直接带出，免掉第二次全量扫描
        // （无名字的轨此前必须走完整条轨；164M 实测占端到端 ~57%）。
        name: events
            .track_name
            .map(scan::decode_midi_text)
            .filter(|n| !n.is_empty()),
        note_count,
        tempo_changes: events.tempo_changes,
        control_events: events.control_events,
        time_signatures: events
            .time_signatures
            .into_iter()
            .map(|ts| {
                (
                    ts.tick,
                    ts.numerator,
                    1u32.wrapping_shl(ts.denominator as u32) as u8,
                )
            })
            .collect(),
        key_signatures: events
            .key_signatures
            .into_iter()
            .map(|ks| (ks.tick, ks.sharps, ks.is_minor))
            .collect(),
        lyrics: events
            .lyrics
            .into_iter()
            .map(|ev| (ev.tick, ev.track, ev.text.to_vec()))
            .collect(),
        markers: events
            .markers
            .into_iter()
            .map(|ev| (ev.tick, ev.track, ev.text.to_vec()))
            .collect(),
        text_events: events
            .text_events
            .into_iter()
            .map(|ev| (ev.tick, ev.track, ev.meta_type, ev.text.to_vec()))
            .collect(),
        sys_ex: events
            .sys_ex
            .into_iter()
            .map(|ev| (ev.tick, ev.track, ev.data.to_vec()))
            .collect(),
        timing: events.timing,
    }
}

/// 逐轨无锁并行地提取并物化全部事件（保留回调的并行契约与轨序）。
fn extract_track_builds(file_bytes: &[u8]) -> LoaderResult<Vec<TrackBuild>> {
    // Tier2：逐轨无锁并行——排序 / ChunkedList 构建 / 文本复制都在工作线程内完成，
    // 仅最终按轨序归并串行。旧实现用全局 Mutex 串行执行这些逐轨工作，
    // 实测占 Pi.mid 端到端墙钟 ~80%（见 docs/research 指导书 §P3 与 fork 的 perf 数据）。
    midly::loader::extract_all_events_per_track_collect_from_bytes(
        file_bytes,
        |_track_idx, events| track_build_from_events(events),
    )
    .map_err(|e| LoaderError::MidiParse(trf!("提取音符失败: {e}", e = e)))
}

/// 按轨序归并逐轨产物。
fn merge_track_builds(track_builds: Vec<TrackBuild>) -> MergedTracks {
    // 按轨序归并。顺序确定：不再依赖并行回调的到达顺序
    // （旧实现在跨轨同 tick 事件上的相对顺序随线程调度变化）。
    // 归并前按精确总量预分配，避免串行归并阶段的倍增重分配抖动。
    let capacity = |f: fn(&TrackBuild) -> usize| track_builds.iter().map(f).sum::<usize>();
    let mut notes: Vec<crate::chunked_list::ChunkedList<NoteEvent>> =
        Vec::with_capacity(track_builds.len());
    let mut track_ports: Vec<u8> = Vec::with_capacity(track_builds.len());
    // TrackName 由快路径逐轨带出（不再做第二次全量扫描），按轨序归并。
    let mut track_names: Vec<Option<String>> = Vec::with_capacity(track_builds.len());
    let mut all_tempo_changes: Vec<(u32, f32)> =
        Vec::with_capacity(capacity(|t| t.tempo_changes.len()));
    let mut all_time_signatures: Vec<(u32, u8, u8)> =
        Vec::with_capacity(capacity(|t| t.time_signatures.len()));
    let mut all_key_signatures: Vec<(u32, i8, bool)> =
        Vec::with_capacity(capacity(|t| t.key_signatures.len()));
    let mut control_events: Vec<midly::loader::PackedControlEvent> =
        Vec::with_capacity(capacity(|t| t.control_events.len()));
    let mut lyrics: Vec<(u32, u16, Vec<u8>)> = Vec::with_capacity(capacity(|t| t.lyrics.len()));
    let mut markers: Vec<(u32, u16, Vec<u8>)> = Vec::with_capacity(capacity(|t| t.markers.len()));
    let mut text_events: Vec<(u32, u16, u8, Vec<u8>)> =
        Vec::with_capacity(capacity(|t| t.text_events.len()));
    let mut sys_ex: Vec<(u32, u16, Vec<u8>)> = Vec::with_capacity(capacity(|t| t.sys_ex.len()));
    let mut total_notes: u64 = 0;
    let mut total_ticks: u32 = 0;
    let mut track_timings: Vec<midly::loader::TrackTiming> = Vec::with_capacity(track_builds.len());

    for track in track_builds {
        total_notes += track.note_count;
        // 时间线语义（产品决策）：取各轨**声明结尾（EOT）的最大值**，
        // 而不是音符结束 tick 的最大值——畸形尾部（EOT 之后的事件）
        // 不得把文件时长拉长。缺 EOT 的轨退化为物理末尾（TrackTiming 内处理）。
        total_ticks = total_ticks.max(track.timing.declared_end_tick());
        track_ports.push(track.port);
        track_names.push(track.name);
        notes.push(track.notes);
        all_tempo_changes.extend(track.tempo_changes);
        control_events.extend(track.control_events);
        all_time_signatures.extend(track.time_signatures);
        all_key_signatures.extend(track.key_signatures);
        lyrics.extend(track.lyrics);
        markers.extend(track.markers);
        text_events.extend(track.text_events);
        sys_ex.extend(track.sys_ex);
        track_timings.push(track.timing);
    }

    MergedTracks {
        notes,
        track_ports,
        track_names,
        all_tempo_changes,
        all_time_signatures,
        all_key_signatures,
        control_events,
        lyrics,
        markers,
        text_events,
        sys_ex,
        total_notes,
        total_ticks,
        track_timings,
    }
}

/// 对归并后的事件做终局排序与默认事件补齐（保持文件序语义）。
fn finalize_event_order(merged: &mut MergedTracks) {
    finalize_sorted_events(&mut merged.all_tempo_changes, |&(t, _)| t, (0u32, 120.0f32));
    finalize_sorted_events(
        &mut merged.all_time_signatures,
        |&(t, _, _)| t,
        (0u32, 4u8, 4u8),
    );
    finalize_sorted_events(
        &mut merged.all_key_signatures,
        |&(t, _, _)| t,
        (0u32, 0i8, false),
    );

    // 稳定排序**只按 tick**：同 tick 内完全保留文件序，不做任何二级重排。
    //
    // 这依赖素材本身把 RPN 选择(CC101/100) 写在数据(CC6/38) 之前、并把 RPN 写在
    // 同 tick 的 PitchBend 之前——否则 PB 会用旧的 bend_sensitivity（默认 2 半音）
    // 而非 RPN 设定的值（如 24）。复刻 yinhe 2026-06-27 13:22 fix(audio)。
    //
    // 【已决策 2026-10-03：保持文件序】素材若把 RPN 的 DataEntry 写在选择之前
    // （同 tick，审计 803 组），此处**有意不纠正**：MIDI 是顺序流，按文件序处理
    // 才是规范语义，与参考实现一致；「修复素材」属素材侧责任，不在解析层做。
    // 加二级序会同时威胁 PB 的相对次序，回归 426d5b2d 修好的弯音偏差
    // （98k 个 PB 中 7 个偏差 >1 半音、最大 62 半音）。
    // 该决策由 `lumino_export::audio::event_stream` 的
    // `test_same_tick_rpn_data_before_select_is_preserved_as_is` 锁死：
    // 改语义必须先让那条变红。
    //
    // 与 `chase.rs` 追齐路径的差异是**有意为之**，不是口径不一致缺陷：追齐不是在
    // 重放文件流，而是把「某 tick 的最终状态」重新下发一遍，那批消息的次序由代码
    // 语义决定（选择必须先于数据，否则数据落到错误参数），故那边用
    // 「选择→其他→DataEntry」rank；本路径只负责忠实还原文件顺序。
    merged.control_events.sort_by_key(|e| e.tick);
    merged.lyrics.sort_by_key(|e| e.0);
    merged.markers.sort_by_key(|e| e.0);
    merged.text_events.sort_by_key(|e| e.0);
    merged.sys_ex.sort_by_key(|e| e.0);
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
            tempo_changes: vec![(0, 120.0)],
            time_signatures: vec![(0, 4, 4)],
            key_signatures: Vec::new(),
            control_events: crate::chunked_list::ChunkedList::new(),
            lyrics: Vec::new(),
            markers: Vec::new(),
            text_events: Vec::new(),
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
    ///
    /// ⚠️ 本方法始终把整份文件读入 `Vec<u8>`（便捷入口，主要供测试与命令行工具使用）。
    /// 编辑器主链路走 `lumino_midi_loader::load_parsed_midi`，其输入层是只读映射
    /// （`crates/audio/midi-loader/src/loader/mmap_file.rs`），不产生整份文件的堆副本。
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
        let (doc, division, notes, _report) =
            Self::from_notes_bytes_with_timing(file_bytes, progress)?;
        Ok((doc, division, notes))
    }

    /// 严格模式加载：任一轨在 EOT 之后仍有事件时返回 [`LoaderError::EotViolation`]。
    ///
    /// 默认加载（[`Self::from_notes_bytes`]）**不受影响**：兼容模式保留 EOT 后事件。
    /// 语义与 [`LoadTimingReport::is_strict_clean`] 一致（悬挂音符不算违规）。
    ///
    /// 成本说明：严格模式先完整构建文档再判定；对畸形大文件会白做一次构建。
    /// 目前真实素材零触发，暂不做「浅扫描预检」。
    pub fn from_notes_bytes_strict(
        file_bytes: &[u8],
        progress: Option<&dyn Fn(f64)>,
    ) -> LoaderResult<(Self, u16, u64, crate::eot::LoadTimingReport)> {
        let out = Self::from_notes_bytes_with_timing(file_bytes, progress)?;
        let violations = out.3.tracks_with_events_after_eot();
        if violations > 0 {
            return Err(LoaderError::EotViolation { tracks: violations });
        }
        Ok(out)
    }

    /// 与 [`Self::from_notes_bytes`] 相同，但额外返回逐轨 EOT / 悬挂音符诊断。
    ///
    /// 报告只含诊断信息（不参与文档语义）；是否提示/裁剪由调用方决定。
    pub fn from_notes_bytes_with_timing(
        file_bytes: &[u8],
        progress: Option<&dyn Fn(f64)>,
    ) -> LoaderResult<(Self, u16, u64, crate::eot::LoadTimingReport)> {
        MemoryMonitor::global().check();

        if let Some(cb) = progress {
            (cb)(0.05);
        }

        let (_, division) = scan::scan_header_info(file_bytes)
            .ok_or_else(|| LoaderError::FileFormat(tr!("无法解析 MIDI 文件头").to_string()))?;

        if let Some(cb) = progress {
            (cb)(0.10);
        }

        // 使用 MIDI 标签追踪解析过程中的密集内存分配
        lumino_diagnostics::memtrace::with_tag(lumino_diagnostics::memtrace::AllocTag::Midi, || {
            if let Some(cb) = progress {
                (cb)(0.15);
            }

            let track_builds = extract_track_builds(file_bytes)?;
            let mut merged = merge_track_builds(track_builds);
            finalize_event_order(&mut merged);

            if let Some(cb) = progress {
                (cb)(0.75);
            }

            let track_count = merged.notes.len() as u16;
            let tracks_manager = TrackManager::new(track_count);

            if let Some(cb) = progress {
                (cb)(0.90);
            }

            let timing_report = crate::eot::LoadTimingReport::from_tracks(merged.track_timings);
            Ok((
                Self {
                    notes: merged.notes,
                    tempo_changes: merged.all_tempo_changes,
                    time_signatures: merged.all_time_signatures,
                    key_signatures: merged.all_key_signatures,
                    control_events: crate::chunked_list::ChunkedList::from_sorted(
                        merged.control_events,
                    ),
                    lyrics: merged.lyrics,
                    markers: merged.markers,
                    text_events: merged.text_events,
                    sys_ex: merged.sys_ex,
                    track_names: merged.track_names,
                    total_ticks: merged.total_ticks,
                    track_count,
                    tracks: tracks_manager,
                    division,
                    track_ports: merged.track_ports,
                    // max_end_tick 缓存：初始 None（脏），首次查询惰性重算
                    track_max_end_ticks: Self::new_track_max_ticks(track_count as usize),
                },
                division,
                merged.total_notes,
                timing_report,
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
