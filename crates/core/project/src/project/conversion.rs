//! LuminoProject 与 MidiDocument 之间的双向转换
//!
//! 新工程格式保存时从 `MidiDocument` 拆分数据，加载时重新组装为 `MidiDocument`
//! 供编辑器、播放器、导出器复用。

use std::path::PathBuf;

use lumino_midi_model::{MidiDocument, NoteEvent, TrackManager};

use crate::{
    LmtrackData, LoadedFileEntry, LoadedFormat, LuminoProject, TrackMeta, TrackSlot,
    TrackVisibilitySer,
};
use lumino_core::error::Result;

impl LuminoProject {
    /// 从 `MidiDocument` 构建 `LuminoProject`
    ///
    /// 将 MidiDocument 中的 per-track 事件拆分为各 `.lmtrack` 文件的数据结构。
    pub fn from_midi_document(doc: &MidiDocument) -> Self {
        let mut project = Self::new("Untitled");
        project.metadata.audio.division = doc.division;
        project.metadata.audio.total_ticks = doc.total_ticks;
        project.metadata.audio.track_count = doc.track_count;
        project.tempo_changes = doc.tempo_changes.clone();
        project.time_signatures = doc.time_signatures.clone();
        project.key_signatures = doc.key_signatures.clone();
        project.lyrics = doc.lyrics.clone();
        project.markers = doc.markers.clone();
        project.sys_ex = doc.sys_ex.clone();
        project.track_names = doc.track_names.clone();

        // 从控制事件中拆分 CC / PC / 弯音
        for ev in &doc.control_events {
            match ev.kind {
                0 => {
                    let (controller, value) = ev.as_control_change();
                    project
                        .control_changes
                        .push((ev.tick, ev.track, ev.channel, controller, value));
                }
                1 => {
                    let program = ev.as_program_change();
                    project
                        .program_changes
                        .push((ev.tick, ev.track, ev.channel, program));
                }
                2 => {
                    // as_pitch_bend 返回 -1.0..1.0 的归一化值，转换为以 8192 为中心的偏移量
                    let normalized = ev.as_pitch_bend();
                    let offset = (normalized * 8192.0).round() as i16;
                    project
                        .pitch_bends
                        .push((ev.tick, ev.track, ev.channel, offset));
                }
                _ => {}
            }
        }

        // 提取每轨事件（含空白音轨：无音符但需保留音轨槽位与元数据，
        // 否则空白音轨不会被保存/加载）
        for track_id in 0..doc.track_count {
            let track_notes = doc.track_notes(track_id as usize);

            // 从 NoteEvent 构造 CompactEvent 音符事件（空白音轨则为空）
            let mut track_events: Vec<lumino_midi_model::compact::CompactEvent> =
                Vec::with_capacity(track_notes.len() * 2);
            for note in track_notes {
                let [on, off] = note.to_compact_events(track_id);
                track_events.push(on);
                track_events.push(off);
            }
            // 稳定排序（保持音符声明顺序）：同 tick 的 NoteOff 必然排在 NoteOn 前
            // （同一音符的 off 先于后续音符的 on 插入），配合回读端 FIFO 配对，
            // 保证同 key 相接/重叠音符的 NoteOn/NoteOff 配对一致。
            // sort_unstable 会打乱同 tick 顺序 → 音符长度偶发错乱。
            track_events.sort_by_key(|e| e.delta_tick());

            // 将绝对 tick 转换为相对 delta_tick，保证 CompactEvent 语义一致
            let mut last_tick = 0_u32;
            for ev in &mut track_events {
                let abs_tick = ev.delta_tick();
                ev.set_delta_tick(abs_tick.saturating_sub(last_tick));
                last_tick = abs_tick;
            }

            // 推断 channel：优先取首个音符事件的通道；空白音轨（无音符）回退到
            // document 的 track_channel（含控制事件通道，最终默认 0）
            let channel = if let Some(ev) = track_events.iter().find(|ev| ev.kind().is_note()) {
                ev.channel()
            } else {
                doc.track_channel(track_id)
            };

            // 推断 max_tick：最后一对音符的结束 tick（绝对值）
            let max_tick = track_events
                .iter()
                .scan(0_u32, |acc, ev| {
                    *acc = acc.saturating_add(ev.delta_tick());
                    Some(*acc)
                })
                .last()
                .unwrap_or(0);

            let name = doc.track_name(track_id as usize).unwrap_or("").to_string();

            // 从 document 音轨视图恢复可见性与 solo（空白音轨通常需要保留
            // 隐藏/静音状态，否则保存→加载后状态丢失）
            let (visibility, solo) = doc
                .tracks
                .get(track_id)
                .map(|view| {
                    (
                        match view.visibility {
                            lumino_midi_model::TrackVisibility::Visible => {
                                TrackVisibilitySer::Visible
                            }
                            lumino_midi_model::TrackVisibility::Muted => TrackVisibilitySer::Muted,
                            lumino_midi_model::TrackVisibility::Hidden => {
                                TrackVisibilitySer::Hidden
                            }
                        },
                        view.solo,
                    )
                })
                .unwrap_or((TrackVisibilitySer::Visible, false));

            let meta = TrackMeta {
                track_id,
                name,
                channel,
                port: doc.track_port(track_id),
                visibility,
                solo,
                is_drum: channel == 9, // MIDI 通道 10 (0-indexed 9) 为鼓组
                max_tick,
            };

            let track_data = LmtrackData::from_compact_events(meta, &track_events);
            project.add_track(track_data);
        }

        // 统计总音符数
        project.metadata.audio.total_notes = project
            .tracks
            .iter()
            .filter_map(|t| match t {
                TrackSlot::Loaded(d) | TrackSlot::Modified(d) => Some(d.note_count),
                TrackSlot::Unloaded { .. } => None,
            })
            .sum();

        project
    }

    /// 将 `LuminoProject` 重建为 `MidiDocument`
    pub fn to_midi_document(&self) -> Result<MidiDocument> {
        let track_count = self.tracks.len().max(1) as u16;
        let mut notes: Vec<lumino_midi_model::ChunkedList<NoteEvent>> = (0..track_count)
            .map(|_| lumino_midi_model::ChunkedList::new())
            .collect();
        let mut total_ticks: u32 = 0;
        let mut next_id: u64 = 1; // 转换路径顺手分配全局唯一 ID
        let mut track_names = Vec::with_capacity(track_count as usize);

        for (idx, slot) in self.tracks.iter().enumerate() {
            let track_data = match slot {
                TrackSlot::Loaded(d) | TrackSlot::Modified(d) => d,
                TrackSlot::Unloaded { .. } => {
                    track_names.push(None);
                    continue;
                }
            };

            track_names.push(Some(track_data.meta.name.clone()));

            let compact_events = track_data.compact_events()?;
            // FIFO 配对：同 key 重叠音符按 NoteOn 顺序匹配 NoteOff
            // （HashMap 单槽会互相覆盖，导致重叠音符长度错乱——变短/变长）
            let mut active: std::collections::HashMap<
                (u8, u8),
                std::collections::VecDeque<(u32, u8)>,
            > = std::collections::HashMap::new();
            let mut current_tick = 0_u32;

            for ev in compact_events {
                current_tick = current_tick.saturating_add(ev.delta_tick());
                let key = ev.param1() as u8;
                let channel = ev.channel();
                let kind = ev.kind();
                let velocity = ev.param2() as u8;

                if kind == lumino_midi_model::compact::EventKind::NoteOn && velocity > 0 {
                    active
                        .entry((key, channel))
                        .or_default()
                        .push_back((current_tick, velocity));
                } else if (kind == lumino_midi_model::compact::EventKind::NoteOff
                    || (kind == lumino_midi_model::compact::EventKind::NoteOn && velocity == 0))
                    && let Some(queue) = active.get_mut(&(key, channel))
                    && let Some((start_tick, note_velocity)) = queue.pop_front()
                {
                    notes[idx].push_back(
                        NoteEvent::new(start_tick, current_tick, key, note_velocity, channel)
                            .with_id(next_id),
                    );
                    next_id += 1;
                    total_ticks = total_ticks.max(current_tick);
                }
            }

            // 未关闭的音符延伸到 max_tick
            if let Some(max_tick) =
                (track_data.meta.max_tick > 0).then_some(track_data.meta.max_tick)
            {
                for ((key, channel), queue) in active {
                    for (start_tick, note_velocity) in queue {
                        notes[idx].push_back(
                            NoteEvent::new(start_tick, max_tick, key, note_velocity, channel)
                                .with_id(next_id),
                        );
                        next_id += 1;
                        total_ticks = total_ticks.max(max_tick);
                    }
                }
            }
        }

        // 重建控制事件
        let mut control_events: Vec<midly::loader::PackedControlEvent> = Vec::new();
        for (tick, track, channel, controller, value) in &self.control_changes {
            control_events.push(midly::loader::PackedControlEvent::control_change(
                *tick,
                *track,
                *channel,
                *controller,
                *value,
            ));
        }
        for (tick, track, channel, program) in &self.program_changes {
            control_events.push(midly::loader::PackedControlEvent::program_change(
                *tick, *track, *channel, *program,
            ));
        }
        for (tick, track, channel, value) in &self.pitch_bends {
            // 将偏移量还原为 0..16383 的原始 pitch bend 值
            let bend = value.saturating_add(8192).clamp(0, 16383) as u16;
            control_events.push(midly::loader::PackedControlEvent::pitch_bend(
                *tick, *track, *channel, bend,
            ));
        }
        control_events.sort_unstable_by_key(|e| e.tick);

        // 补齐缺失的 track_names
        track_names.resize_with(track_count as usize, || None);

        let mut doc = MidiDocument {
            notes,
            next_note_id: next_id,
            tempo_changes: self.tempo_changes.clone(),
            time_signatures: self.time_signatures.clone(),
            key_signatures: self.key_signatures.clone(),
            control_events: lumino_midi_model::ChunkedList::from_sorted(control_events),
            lyrics: self.lyrics.clone(),
            markers: self.markers.clone(),
            sys_ex: self.sys_ex.clone(),
            track_names,
            total_ticks: total_ticks.max(self.metadata.audio.total_ticks),
            track_count,
            tracks: TrackManager::new(track_count),
            division: self.metadata.audio.division,
            track_ports: vec![0u8; track_count as usize],

            track_max_end_ticks: lumino_midi_model::MidiDocument::new_track_max_ticks(
                track_count as usize,
            ),
        };

        // 恢复每轨可见性与 solo（来自 lmtrack 元数据），保证隐藏/静音等音轨状态
        // 在保存→加载往返后不丢失（空白音轨的状态亦在此保留）
        for (idx, slot) in self.tracks.iter().enumerate() {
            let (TrackSlot::Loaded(d) | TrackSlot::Modified(d)) = slot else {
                continue;
            };
            let visibility = match d.meta.visibility {
                TrackVisibilitySer::Visible => lumino_midi_model::TrackVisibility::Visible,
                TrackVisibilitySer::Muted => lumino_midi_model::TrackVisibility::Muted,
                TrackVisibilitySer::Hidden => lumino_midi_model::TrackVisibility::Hidden,
            };
            doc.tracks.set_visibility(idx as u16, visibility);
            doc.tracks.set_solo(idx as u16, d.meta.solo);
        }

        Ok(doc)
    }

    /// 记录一个导入的外部 MIDI 文件
    pub fn record_loaded_midi(
        &mut self,
        id: impl Into<String>,
        original_name: impl Into<String>,
        storage_path: impl Into<PathBuf>,
        imported_at: impl Into<String>,
    ) {
        self.loaded_files.push(LoadedFileEntry {
            id: id.into(),
            original_name: original_name.into(),
            format: LoadedFormat::Mid,
            imported_at: imported_at.into(),
            storage_path: storage_path.into(),
        });
    }
}

/// 为 `Vec<TrackSlot>` 提供便捷方法，避免直接暴露内部字段
pub trait TrackSlotVecExt {
    /// 获取已加载或已修改的音轨数据
    fn loaded_data(&self) -> impl Iterator<Item = (u16, &LmtrackData)>;
}

impl TrackSlotVecExt for Vec<TrackSlot> {
    fn loaded_data(&self) -> impl Iterator<Item = (u16, &LmtrackData)> {
        self.iter()
            .enumerate()
            .filter_map(|(idx, slot)| match slot {
                TrackSlot::Loaded(d) | TrackSlot::Modified(d) => Some((idx as u16, d)),
                TrackSlot::Unloaded { .. } => None,
            })
    }
}

#[cfg(test)]
#[path = "conversion/tests.rs"]
mod tests;
