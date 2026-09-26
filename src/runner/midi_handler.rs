use std::sync::Arc;

use lumino_midi_loader::ParsedMidi;

/// MIDI 处理器
pub struct MidiHandler;

impl MidiHandler {
    pub fn new() -> Self {
        Self
    }

    /// 将 MIDI 数据导入到编辑器
    ///
    /// 2026-08 单一权威源改造：`parsed` 以所有权传入，`MidiDocument` 从
    /// `ParsedMidi.document`（`Option<Arc<MidiDocument>>`）中零拷贝拆出
    /// （`Arc::try_unwrap`，事件传递路径上 Arc 唯一），随后所有权移交给
    /// UI 的 `EditorData.document`。调用方须以所有权传递（见 load.rs）。
    pub fn import_midi_to_editor(&self, ui: &mut lumino_ui::Host, parsed: ParsedMidi) {
        ui.reset_playback_manager();

        // 从 ParsedMidi 中移出 MidiDocument（Arc::try_unwrap 零拷贝）
        let Some(document) = parsed.document.and_then(|arc| Arc::try_unwrap(arc).ok()) else {
            // LMPJ 文件加载时已同步构建 MidiDocument，理论上不应走到此路径
            tracing::warn!("MIDI 没有 document，无法导入");
            return;
        };
        tracing::info!("导入 MIDI 文档：{} 音轨", document.track_count());
        let t_total = std::time::Instant::now();

        let track_count = document.track_count();
        let total_ticks = document.total_ticks();
        let mut track_infos = Vec::with_capacity(track_count);

        // 只收集音轨信息（名称、音符数），不预加载音符到 track_notes
        // 音符将在首次渲染或切换音轨时从 MidiDocument 懒加载
        // 这样可以避免 track_notes + MidiDocument 两份数据共存导致内存翻倍
        // 注意：使用 track_note_count 而非 get_track_notes 以避免全量提取
        let t_infos = std::time::Instant::now();
        for track_idx in 0..track_count {
            let note_count = document.track_note_count(track_idx as u16);
            let track_name = document.track_name(track_idx).map(|s| s.to_string());
            let channel = document.track_channel(track_idx as u16);
            let port = document.track_port(track_idx as u16);
            track_infos.push((track_idx, track_name, note_count, channel, port));
        }
        let t_infos = t_infos.elapsed();

        let t_tracks = std::time::Instant::now();
        ui.set_ppq(parsed.info.division);
        ui.update_tracks(&track_infos);
        let t_tracks = t_tracks.elapsed();

        // 从预存储的 tempo_changes 加载（在 document move 进 UI 之前借用）
        let t_tempo = std::time::Instant::now();
        let tempo_ui: Vec<(u32, u32)> = document
            .tempo_changes
            .iter()
            .map(|&(tick, bpm)| {
                let microseconds = if bpm > 0.0 {
                    lumino_midi_loader::bpm_to_tempo(bpm as f64)
                } else {
                    lumino_midi_loader::constants::DEFAULT_TEMPO_MICROS
                };
                (tick, microseconds)
            })
            .collect();
        let t_tempo = t_tempo.elapsed();

        // REND-003：删除"提取首轨 → 回写"往返（原 `get_track_notes` + `load_track_notes`）。
        // 该往返在千万级音轨上是 O(音符数) 的多遍拷贝（TrackNoteView 无 id → 回写时
        // `replace_track_notes` 还会给首轨重新分配全部 ID），是加载后主线程长时间
        // "未响应"的根因；单一权威源架构下编辑器/渲染均直接读 `document`，
        // 切到首轨（`set_current_track`）即可，无需预加载。
        let first_track_idx = track_infos
            .iter()
            .find(|(_, _, note_count, _, _)| *note_count > 0)
            .map(|(first_track_idx, _, _, _, _)| *first_track_idx);

        // 将 MidiDocument 所有权移交给编辑器（单一权威源，零拷贝）
        let t_setdoc = std::time::Instant::now();
        ui.set_midi_document(document);
        let t_setdoc = t_setdoc.elapsed();

        if !tempo_ui.is_empty() {
            ui.load_tempo_changes(tempo_ui);
        }

        let t_first = std::time::Instant::now();
        if let Some(first_track_idx) = first_track_idx {
            ui.set_current_track(first_track_idx, false);
        }
        let t_first = t_first.elapsed();

        tracing::info!(
            "加载完成: {} 音轨, {} ticks, 音符已加载（收集轨信息 {:?} / update_tracks {:?} / \
             tempo {:?} / set_midi_document {:?} / 切首轨 {:?} / 合计 {:?}）",
            track_count,
            total_ticks,
            t_infos,
            t_tracks,
            t_tempo,
            t_setdoc,
            t_first,
            t_total.elapsed()
        );

        // 按需识别小节长度，并向后拓展 100 小节作为默认空间
        let ppq = parsed.info.division;
        let extra_measures = 100u32;
        let extra_ticks = (ppq as u32) * 4 * extra_measures;
        let total_ticks = (parsed.info.duration_ticks as f32).max(1.0) + extra_ticks as f32;
        ui.set_total_ticks(total_ticks);
    }
}
