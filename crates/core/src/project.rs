//! 工程数据模块 — Lumino 项目文件格式核心
//!
//! 支持两种形态：
//! - 文件夹形式（`.lmpj` 包文件夹）
//! - 单文件形式（`.lmpj` 归档文件）

pub mod archive;
pub mod data_formats;
pub mod folder;
pub mod loaded;
pub mod metadata;
pub mod track;

pub use data_formats::{LmctlData, LmnamesData, LmsigData, LmtempData};
pub use metadata::ProjectMetadata;
pub use track::{LmtrackData, LmtrackHeader, TrackMeta, TrackVisibilitySer};

use std::path::PathBuf;

/// 工程数据（内存中表示）
#[derive(Debug)]
pub struct LuminoProject {
    /// 元数据
    pub metadata: ProjectMetadata,
    /// 音轨数据（懒加载：未修改的音轨可保持磁盘映射）
    pub tracks: Vec<TrackSlot>,
    /// 全局速度变化
    pub tempo_changes: Vec<(u32, f32)>,
    /// 拍号变化
    pub time_signatures: Vec<(u32, u8, u8)>,
    /// 调号变化
    pub key_signatures: Vec<(u32, i8, bool)>,
    /// 控制事件
    pub control_changes: Vec<(u32, u16, u8, u8, u8)>,
    /// 程序变更
    pub program_changes: Vec<(u32, u16, u8, u8)>,
    /// 导入的外部文件
    pub loaded_files: Vec<LoadedFileEntry>,
}

/// 音轨槽（支持懒加载）
#[derive(Debug)]
pub enum TrackSlot {
    /// 未加载（仅在文件中有数据）
    Unloaded { track_id: u16, path: PathBuf },
    /// 已加载到内存
    Loaded(LmtrackData),
    /// 已修改（需要保存）
    Modified(LmtrackData),
}

/// 导入文件条目
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LoadedFileEntry {
    pub id: String,
    pub original_name: String,
    pub format: LoadedFormat,
    pub imported_at: String,
    pub storage_path: PathBuf,
}

/// 导入文件格式
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LoadedFormat {
    Mid,
    Dms,
    Lmpj,
}

impl LuminoProject {
    /// 创建空工程
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            metadata: ProjectMetadata::default_with_name(name),
            tracks: Vec::new(),
            tempo_changes: Vec::new(),
            time_signatures: Vec::new(),
            key_signatures: Vec::new(),
            control_changes: Vec::new(),
            program_changes: Vec::new(),
            loaded_files: Vec::new(),
        }
    }

    /// 获取已加载的音轨数量
    pub fn loaded_track_count(&self) -> usize {
        self.tracks
            .iter()
            .filter(|t| matches!(t, TrackSlot::Loaded(_) | TrackSlot::Modified(_)))
            .count()
    }

    /// 获取指定音轨（如果已加载）
    pub fn get_track(&self, track_id: u16) -> Option<&LmtrackData> {
        self.tracks
            .get(track_id as usize)
            .and_then(|slot| match slot {
                TrackSlot::Loaded(data) | TrackSlot::Modified(data) => Some(data),
                TrackSlot::Unloaded { .. } => None,
            })
    }

    /// 获取指定音轨（可变）
    pub fn get_track_mut(&mut self, track_id: u16) -> Option<&mut LmtrackData> {
        self.tracks
            .get_mut(track_id as usize)
            .and_then(|slot| match slot {
                TrackSlot::Loaded(data) | TrackSlot::Modified(data) => Some(data),
                TrackSlot::Unloaded { .. } => None,
            })
    }

    /// 标记指定音轨为已修改
    pub fn mark_track_modified(&mut self, track_id: u16) {
        if let Some(slot) = self.tracks.get_mut(track_id as usize)
            && let TrackSlot::Loaded(data) = slot
        {
            // 取出数据并标记为 Modified
            let data = std::mem::replace(
                data,
                LmtrackData::from_compact_events(
                    TrackMeta {
                        track_id: 0,
                        name: String::new(),
                        channel: 0,
                        port: 0,
                        visibility: TrackVisibilitySer::Visible,
                        solo: false,
                        is_drum: false,
                        max_tick: 0,
                    },
                    &[],
                ),
            );
            *slot = TrackSlot::Modified(data);
        }
    }

    /// 添加音轨
    pub fn add_track(&mut self, data: LmtrackData) {
        let track_id = data.meta.track_id;
        let idx = track_id as usize;
        if idx >= self.tracks.len() {
            self.tracks.resize_with(idx + 1, || TrackSlot::Unloaded {
                track_id: 0,
                path: PathBuf::new(),
            });
        }
        self.tracks[idx] = TrackSlot::Modified(data);
    }
}

impl LuminoProject {
    /// 从 `MidiDocument` 构建 `LuminoProject`
    ///
    /// 关键路径：用户打开 MIDI 文件后，需要能保存为新格式。
    /// 将 MidiDocument 中的 per-track 事件拆分为各 `.lmtrack` 文件的数据结构。
    pub fn from_midi_document(doc: &crate::midi::document::MidiDocument) -> Self {
        let mut project = Self::new("Untitled");
        project.metadata.audio.division = 480; // 默认值，应由调用方填充
        project.metadata.audio.total_ticks = doc.total_ticks;
        project.metadata.audio.track_count = doc.track_count;
        project.tempo_changes = doc.tempo_changes.clone();

        // 提取每轨事件
        for track_id in 0..doc.track_count {
            let (start, end) = doc.track_events_range(track_id);
            if start >= end {
                continue;
            }

            let track_events: Vec<_> = doc.events[start..end]
                .iter()
                .filter(|ev| !ev.kind().is_meta()) // 过滤全局 meta 事件
                .copied()
                .collect();

            if track_events.is_empty() {
                continue;
            }

            // 推断 channel：取第一个音符事件的 channel
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

        // TODO: 从 MidiDocument.control_events 中提取 control_changes / program_changes
        // 需要了解 midly::loader::PackedControlEvent 的具体字段

        project
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_new() {
        let project = LuminoProject::new("Test");
        assert_eq!(project.metadata.project.name, "Test");
        assert!(project.tracks.is_empty());
        assert!(project.tempo_changes.is_empty());
    }

    #[test]
    fn test_add_track() {
        let mut project = LuminoProject::new("Test");
        let data = LmtrackData::from_compact_events(
            TrackMeta {
                track_id: 0,
                name: "Piano".into(),
                channel: 0,
                port: 0,
                visibility: TrackVisibilitySer::Visible,
                solo: false,
                is_drum: false,
                max_tick: 100,
            },
            &[],
        );
        project.add_track(data);
        assert_eq!(project.tracks.len(), 1);
        assert!(matches!(project.tracks[0], TrackSlot::Modified(_)));
    }

    #[test]
    fn test_get_track() {
        let mut project = LuminoProject::new("Test");
        let data = LmtrackData::from_compact_events(
            TrackMeta {
                track_id: 0,
                name: "Piano".into(),
                channel: 0,
                port: 0,
                visibility: TrackVisibilitySer::Visible,
                solo: false,
                is_drum: false,
                max_tick: 100,
            },
            &[],
        );
        project.add_track(data);

        assert!(project.get_track(0).is_some());
        assert!(project.get_track(99).is_none());
    }
}
