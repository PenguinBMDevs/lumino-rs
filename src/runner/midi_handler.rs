use super::midi_parser::parse_midi_events_to_notes;
use lumino_core::ParsedMidi;

/// MIDI 处理器
pub struct MidiHandler {
    // MIDI 处理相关状态（如果有）
}

impl MidiHandler {
    pub fn new() -> Self {
        Self {}
    }

    /// 将 MIDI 数据导入到编辑器
    pub fn import_midi_to_editor(&self, ui: &mut lumino_ui::Host, parsed: &ParsedMidi) {
        use lumino_core::MidiEvent;

        // 获取 memory_manager
        if let Some(memory_manager_arc) = parsed.memory_manager.as_ref() {
            // 有 memory_manager，使用原有逻辑
            let mut memory_manager: std::sync::MutexGuard<lumino_core::MidiMemoryManager> =
                match memory_manager_arc.lock() {
                    Ok(mgr) => mgr,
                    Err(e) => {
                        tracing::error!("无法锁定 memory_manager: {}", e);
                        return;
                    }
                };

            // 收集所有音轨信息
            let mut track_infos = Vec::new();
            let summaries = memory_manager.all_summaries().to_vec();

            for summary in &summaries {
                let track_idx = summary.track_index;

                // 获取音轨事件以读取音轨名称
                let track_name: Option<String> =
                    match memory_manager.get_track_events_full(track_idx) {
                        Ok(events) => {
                            // 查找 TrackName 事件
                            events.iter().find_map(|e: &MidiEvent| {
                                if let MidiEvent::TrackName { name, .. } = e {
                                    Some(name.clone())
                                } else {
                                    None
                                }
                            })
                        }
                        Err(e) => {
                            tracing::warn!("无法获取音轨 {} 事件: {}", track_idx, e);
                            None
                        }
                    };

                track_infos.push((track_idx, track_name, summary.note_count));
            }

            ui.set_ppq(parsed.info.division);

            // 更新 UI 音轨列表
            ui.update_tracks(&track_infos);

            // 提取并加载 Tempo 变化事件
            self.load_tempo_changes_from_memory_manager(&mut memory_manager, ui);

            // 预加载所有音轨的音符到 track_notes（供洋葱皮使用）
            tracing::info!("Pre-loading all tracks for onion skin...");
            for (track_idx, _, note_count) in &track_infos {
                if *note_count > 0 {
                    // 加载音符但不切换到该音轨（只保存到 track_notes）
                    self.preload_track_for_onion_skin(&mut memory_manager, *track_idx, ui);
                }
            }

            // 加载第一个有音符的音轨到编辑器（实际显示）
            if let Some((first_track_idx, _, _)) = track_infos
                .iter()
                .find(|(_, _, note_count)| *note_count > 0)
            {
                self.load_track_to_editor(&mut memory_manager, *first_track_idx, ui);
            }
        } else if let Some(midi_data) = parsed.midi_data.as_ref() {
            // 没有 memory_manager 但有 midi_data，从 midi_data 解析音符
            tracing::info!("从 midi_data 解析音符数据");
            self.import_midi_data_to_editor(midi_data, parsed.info.track_count as usize, ui);
        } else {
            tracing::warn!("MIDI 没有 memory_manager 也没有 midi_data，无法导入音符");
            return;
        }

        // 更新编辑器总 ticks
        let total_ticks = parsed.info.duration_ticks as f32;
        ui.set_total_ticks(total_ticks);
    }

    /// 从 MIDI 字节流导入音符到编辑器
    pub fn import_midi_data_to_editor(
        &self,
        midi_data: &[u8],
        _track_count: usize,
        ui: &mut lumino_ui::Host,
    ) {
        use super::midi_parser::parse_smf_to_notes;
        use midly::Smf;

        // 解析 MIDI 数据
        let smf = match Smf::parse(midi_data) {
            Ok(smf) => smf,
            Err(e) => {
                tracing::error!("解析 MIDI 数据失败: {}", e);
                return;
            }
        };

        // 使用通用解析函数收集音轨信息和音符
        let ppq = match smf.header.timing {
            midly::Timing::Metrical(ppq) => ppq.as_int(),
            _ => 1920,
        };
        ui.set_ppq(ppq);

        let (track_infos, track_notes_map) = parse_smf_to_notes(&smf);

        // 更新 UI 音轨列表
        ui.update_tracks(&track_infos);

        // 将所有音轨的音符导入编辑器
        for (track_idx, notes) in track_notes_map {
            ui.load_track_notes(track_idx, &notes);
            tracing::info!(
                "从 midi_data 导入音轨 {}，共 {} 个音符",
                track_idx,
                notes.len()
            );
        }

        // 加载第一个有音符的音轨到编辑器（实际显示）
        if let Some((first_track_idx, _, _)) = track_infos
            .iter()
            .find(|(_, _, note_count)| *note_count > 0)
        {
            ui.set_current_track(*first_track_idx);
        }
    }

    /// 加载指定音轨的音符到编辑器
    pub fn load_track_to_editor(
        &self,
        memory_manager: &mut lumino_core::MidiMemoryManager,
        track_idx: usize,
        ui: &mut lumino_ui::Host,
    ) {
        use lumino_core::MidiEvent;

        tracing::info!("load_track_to_editor: track_idx={}", track_idx);

        let events = match memory_manager.get_track_events_full(track_idx) {
            Ok(events) => {
                tracing::info!("  got {} events from track {}", events.len(), track_idx);
                // 统计音符事件
                let note_on_count = events
                    .iter()
                    .filter(|e| matches!(e, MidiEvent::NoteOn { .. }))
                    .count();
                let note_off_count = events
                    .iter()
                    .filter(|e| matches!(e, MidiEvent::NoteOff { .. }))
                    .count();
                tracing::info!("  NoteOn: {}, NoteOff: {}", note_on_count, note_off_count);
                events
            }
            Err(e) => {
                tracing::error!("加载音轨 {} 失败: {}", track_idx, e);
                return;
            }
        };

        // 使用通用解析函数构建音符列表
        let notes = parse_midi_events_to_notes(&events);

        // 更新编辑器音符（使用新的函数，同时保存到 track_notes 供洋葱皮使用）
        ui.load_track_notes(track_idx, &notes);

        tracing::info!("音轨 {} 已加载，共 {} 个音符", track_idx, notes.len());
    }

    /// 预加载音轨音符到 track_notes（用于洋葱皮，不切换到该音轨）
    pub fn preload_track_for_onion_skin(
        &self,
        memory_manager: &mut lumino_core::MidiMemoryManager,
        track_idx: usize,
        ui: &mut lumino_ui::Host,
    ) {
        tracing::debug!("Preloading track {} for onion skin", track_idx);

        let events = match memory_manager.get_track_events_full(track_idx) {
            Ok(events) => events,
            Err(e) => {
                tracing::warn!("预加载音轨 {} 失败: {}", track_idx, e);
                return;
            }
        };

        // 使用通用解析函数构建音符列表
        let notes = parse_midi_events_to_notes(&events);

        // 只保存到 track_notes，不切换到该音轨
        if !notes.is_empty() {
            ui.load_track_notes_for_onion_skin(track_idx, &notes);
            tracing::debug!(
                "Preloaded track {} with {} notes for onion skin",
                track_idx,
                notes.len()
            );
        }
    }

    /// 从 memory_manager 提取并加载 Tempo 变化事件
    fn load_tempo_changes_from_memory_manager(
        &self,
        memory_manager: &mut lumino_core::MidiMemoryManager,
        ui: &mut lumino_ui::Host,
    ) {
        use lumino_core::MidiEvent;

        let mut tempo_changes = Vec::new();

        // 遍历所有音轨，提取 Tempo 事件
        let summaries = memory_manager.all_summaries().to_vec();
        for summary in &summaries {
            let track_idx = summary.track_index;

            if let Ok(events) = memory_manager.get_track_events_full(track_idx) {
                for event in events {
                    if let MidiEvent::Tempo { tick, tempo, .. } = event {
                        tempo_changes.push((tick, tempo));
                    }
                }
            }
        }

        // 按 tick 排序
        tempo_changes.sort_by_key(|(tick, _)| *tick);

        // 如果没有 tempo 事件，添加默认的 120 BPM
        if tempo_changes.is_empty() {
            // 120 BPM = 500000 microseconds per quarter note
            tempo_changes.push((0, 500000));
            tracing::info!("No tempo events found, using default 120 BPM");
        } else {
            tracing::info!(
                "Loaded {} tempo changes from MIDI file",
                tempo_changes.len()
            );
        }

        // 加载到 UI
        ui.load_tempo_changes(tempo_changes);
    }
}
