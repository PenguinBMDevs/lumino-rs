use lumino_midi_loader::ParsedMidi;

/// MIDI 处理器
pub struct MidiHandler;

impl MidiHandler {
    pub fn new() -> Self {
        Self
    }

    /// 将 MIDI 数据导入到编辑器
    pub fn import_midi_to_editor(&self, ui: &mut lumino_ui::Host, parsed: &ParsedMidi) {
        ui.reset_playback_manager();

        let Some(doc_arc) = parsed.document.as_ref() else {
            // LMPJ 文件加载时已同步构建 MidiDocument，理论上不应走到此路径
            tracing::warn!("MIDI 没有 document，无法导入");
            return;
        };
        let document = doc_arc.as_ref();
        tracing::info!("导入 MIDI 文档：{} 音轨", document.track_count());

        let track_count = document.track_count();
        let mut track_infos = Vec::with_capacity(track_count);

        // 只收集音轨信息（名称、音符数），不预加载音符到 track_notes
        // 音符将在首次渲染或切换音轨时从 MidiDocument 懒加载
        // 这样可以避免 track_notes + MidiDocument 两份数据共存导致内存翻倍
        // 注意：使用 track_note_count 而非 get_track_notes 以避免全量提取
        for track_idx in 0..track_count {
            let note_count = document.track_note_count(track_idx as u16);
            let track_name = document.track_name(track_idx).map(|s| s.to_string());
            let channel = document.track_channel(track_idx as u16);
            track_infos.push((track_idx, track_name, note_count, channel));
        }

        ui.set_ppq(parsed.info.division);
        ui.update_tracks(&track_infos);

        // 将 MidiDocument 传递给编辑器供懒加载使用
        // Arc::clone 是 O(1) 引用计数递增，避免深拷贝 120MB+ 事件数据
        ui.set_midi_document(std::sync::Arc::clone(doc_arc));

        // 从预存储的 tempo_changes 加载
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
        if !tempo_ui.is_empty() {
            ui.load_tempo_changes(tempo_ui);
        }

        // 加载第一个有音符的音轨到编辑器（实际显示 + 懒加载缓存）
        if let Some((first_track_idx, _, _, _)) = track_infos
            .iter()
            .find(|(_, _, note_count, _)| *note_count > 0)
        {
            let first_notes = document.get_track_notes(*first_track_idx as u16);
            ui.load_track_notes(*first_track_idx, &first_notes);
            ui.set_current_track(*first_track_idx, false);
        }

        tracing::info!(
            "加载完成: {} 音轨, {} ticks, 音符已加载",
            track_count,
            document.total_ticks()
        );

        // 按需识别小节长度，并向后拓展 100 小节作为默认空间
        let ppq = parsed.info.division;
        let extra_measures = 100u32;
        let extra_ticks = (ppq as u32) * 4 * extra_measures;
        let total_ticks = (parsed.info.duration_ticks as f32).max(1.0) + extra_ticks as f32;
        ui.set_total_ticks(total_ticks);
    }
}
