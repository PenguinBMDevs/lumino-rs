//! LuminoProject 与 MidiDocument 之间的双向转换
//!
//! 新工程格式保存时从 `MidiDocument` 拆分数据，加载时重新组装为 `MidiDocument`
//! 供编辑器、播放器、导出器复用。

use std::path::PathBuf;

use lumino_midi_model::{MidiDocument, NoteEvent, TrackManager};

use crate::project::{
    LoadedFileEntry, LoadedFormat, LuminoProject, TrackMeta, TrackSlot, TrackVisibilitySer,
    track::LmtrackData,
};
use crate::{CoreError, Result};

impl LuminoProject {
    /// 从 `MidiDocument` 构建 `LuminoProject`
    ///
    /// 将 MidiDocument 中的 per-track 事件拆分为各 `.lmtrack` 文件的数据结构。
    pub fn from_midi_document(doc: &MidiDocument) -> Self {
        let mut project = Self::new("Untitled");
        project.metadata.audio.division = 480; // 默认值，由调用方后续覆盖
        project.metadata.audio.total_ticks = doc.total_ticks;
        project.metadata.audio.track_count = doc.track_count;
        project.tempo_changes = doc.tempo_changes.clone();
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

        // 提取每轨事件
        for track_id in 0..doc.track_count {
            let track_notes = doc.track_notes(track_id as usize);
            if track_notes.is_empty() {
                continue;
            }

            // 从 NoteEvent 构造 CompactEvent，并过滤出音符事件
            let mut track_events: Vec<lumino_midi_model::compact::CompactEvent> =
                Vec::with_capacity(track_notes.len() * 2);
            for note in track_notes {
                let [on, off] = note.to_compact_events(track_id);
                track_events.push(on);
                track_events.push(off);
            }
            track_events.sort_unstable_by_key(|e| e.delta_tick());

            // 推断 channel：取第一个音符事件的通道
            let channel = track_events
                .iter()
                .find(|ev| ev.kind().is_note())
                .map(|ev| ev.channel())
                .unwrap_or(0);

            // 推断 max_tick
            let max_tick = track_events.last().map(|ev| ev.delta_tick()).unwrap_or(0);

            let name = doc.track_name(track_id as usize).unwrap_or("").to_string();

            let meta = TrackMeta {
                track_id,
                name,
                channel,
                port: 0,
                visibility: TrackVisibilitySer::Visible,
                solo: false,
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
        let mut notes: Vec<Vec<NoteEvent>> = vec![Vec::new(); track_count as usize];
        let mut total_ticks: u32 = 0;
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
            let mut event_iter = compact_events.peekable();
            while let Some(on) = event_iter.next() {
                let off = event_iter.next().ok_or_else(|| {
                    CoreError::FileFormat(format!("track {}: NoteOn without matching NoteOff", idx))
                })?;

                if !matches!(on.kind(), lumino_midi_model::compact::EventKind::NoteOn) {
                    return Err(CoreError::FileFormat(format!(
                        "track {}: expected NoteOn, got {:?}",
                        idx,
                        on.kind()
                    )));
                }
                if !matches!(off.kind(), lumino_midi_model::compact::EventKind::NoteOff) {
                    return Err(CoreError::FileFormat(format!(
                        "track {}: expected NoteOff after NoteOn, got {:?}",
                        idx,
                        off.kind()
                    )));
                }

                let start_tick = on.delta_tick();
                let end_tick = off.delta_tick();
                total_ticks = total_ticks.max(end_tick);

                notes[idx].push(NoteEvent::new(
                    start_tick,
                    end_tick,
                    on.param1() as u8,
                    on.param2() as u8,
                    on.channel(),
                ));
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

        Ok(MidiDocument {
            notes,
            tempo_changes: self.tempo_changes.clone(),
            control_events,
            track_names,
            total_ticks: total_ticks.max(self.metadata.audio.total_ticks),
            track_count,
            tracks: TrackManager::new(track_count),
        })
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
mod tests {
    use super::*;
    use lumino_midi_model::compact::{CompactEvent, EventKind};

    fn make_test_document() -> MidiDocument {
        MidiDocument {
            notes: vec![vec![NoteEvent::new(0, 480, 60, 100, 0)]],
            tempo_changes: vec![(0, 120.0)],
            control_events: vec![midly::loader::PackedControlEvent::control_change(
                0, 0, 0, 7, 100,
            )],
            track_names: vec![Some("Piano".into())],
            total_ticks: 480,
            track_count: 1,
            tracks: TrackManager::new(1),
        }
    }

    #[test]
    fn test_from_midi_document() {
        let doc = make_test_document();
        let project = LuminoProject::from_midi_document(&doc);

        assert_eq!(project.metadata.audio.total_ticks, 480);
        assert_eq!(project.metadata.audio.track_count, 1);
        assert_eq!(project.tempo_changes.len(), 1);
        assert_eq!(project.control_changes.len(), 1);
        assert_eq!(project.loaded_track_count(), 1);

        let track = project.get_track(0).expect("音轨 0 应已加载");
        assert_eq!(track.meta.name, "Piano");
        assert_eq!(track.note_count, 1);
    }

    #[test]
    fn test_to_midi_document_roundtrip() {
        let doc = make_test_document();
        let project = LuminoProject::from_midi_document(&doc);
        let rebuilt = project.to_midi_document().expect("重建 MidiDocument 失败");

        assert_eq!(rebuilt.track_count, 1);
        assert_eq!(rebuilt.total_ticks, 480);
        assert_eq!(rebuilt.notes[0].len(), 1);
        assert_eq!(rebuilt.tempo_changes.len(), 1);
        assert_eq!(rebuilt.control_events.len(), 1);
        assert_eq!(rebuilt.track_names[0], Some("Piano".into()));
    }

    #[test]
    fn test_compact_event_roundtrip() {
        let mut project = LuminoProject::new("Test");
        let events = vec![
            CompactEvent::new(0, 0, EventKind::NoteOn, 0, 60, 100),
            CompactEvent::new(480, 0, EventKind::NoteOff, 0, 60, 0),
        ];
        let data = LmtrackData::from_compact_events(
            TrackMeta {
                track_id: 0,
                name: "Test".into(),
                channel: 0,
                port: 0,
                visibility: TrackVisibilitySer::Visible,
                solo: false,
                is_drum: false,
                max_tick: 480,
            },
            &events,
        );
        project.add_track(data);

        let doc = project.to_midi_document().expect("重建失败");
        assert_eq!(doc.notes[0].len(), 1);
        assert_eq!(doc.notes[0][0].start_tick, 0);
        assert_eq!(doc.notes[0][0].end_tick, 480);
        assert_eq!(doc.notes[0][0].key, 60);
        assert_eq!(doc.notes[0][0].velocity, 100);
    }
}
