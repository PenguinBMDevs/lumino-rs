//! 工程数据模块 — Lumino 项目文件格式核心
//!
//! 支持两种形态：
//! - 文件夹形式（`.lmpj` 包文件夹）
//! - 单文件形式（`.lmpj` 归档文件）

pub mod archive;
pub mod conversion;
pub mod data_formats;
pub mod deleted_track;
pub mod folder;
pub mod load;
pub mod metadata;
pub mod save;
pub mod track;

pub use data_formats::{LmctlData, LmnamesData, LmsigData, LmsyxData, LmtempData, LmtxtData};
pub use deleted_track::{
    DeletedNote, DeletedTrackData, DeletedTrackEntry, DeletedTrackMetadata, delete_permanently,
    list_deleted_tracks, load_deleted_track, save_deleted_track,
};
pub use metadata::ProjectMetadata;
pub use track::{LmtrackData, LmtrackHeader, TrackMeta, TrackVisibilitySer};

use std::path::PathBuf;

/// 工程数据（内存中表示）
#[derive(Debug, Clone)]
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
    /// 弯音事件（value 为以 8192 为中心的偏移量）
    pub pitch_bends: Vec<(u32, u16, u8, i16)>,
    /// 歌词文本事件
    pub lyrics: Vec<(u32, u16, Vec<u8>)>,
    /// 标记文本事件
    pub markers: Vec<(u32, u16, Vec<u8>)>,
    /// SysEx 事件
    pub sys_ex: Vec<(u32, u16, Vec<u8>)>,
    /// 音轨名称（索引 = track_id）
    pub track_names: Vec<Option<String>>,
    /// 导入的外部文件
    pub loaded_files: Vec<LoadedFileEntry>,
}

/// 音轨槽（支持懒加载）
#[derive(Debug, Clone)]
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
            pitch_bends: Vec::new(),
            lyrics: Vec::new(),
            markers: Vec::new(),
            sys_ex: Vec::new(),
            track_names: Vec::new(),
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

    /// 用速度点列表覆盖全局速度变化（tick, BPM）
    ///
    /// UI 中速度编辑的权威源是 `tempo_points`（工程设置对话框 / 速度面板），
    /// 而保存链路 `from_midi_document` 只读 `doc.tempo_changes`（加载时的原始值）。
    /// 两者不同步会导致用户修改的 tempo 保存后丢失（回落到默认 120 BPM），
    /// 因此所有保存/导出出口在构建 `LuminoProject` 后必须调用本方法覆盖。
    pub fn apply_tempo_points(&mut self, points: impl IntoIterator<Item = (f32, f64)>) {
        self.tempo_changes = points
            .into_iter()
            .map(|(tick, bpm)| (tick.max(0.0) as u32, bpm as f32))
            .collect();
    }

    /// 设置累计创作时间（秒），写入 `metadata.stats.working_time_seconds`
    ///
    /// 所有保存/导出出口在构建 `LuminoProject` 后必须调用本方法，
    /// 否则累计创作时间不会随工程文件持久化（跨会话丢失）。
    pub fn set_working_time_seconds(&mut self, secs: f64) {
        self.metadata.stats = Some(crate::project::metadata::StatsMetadata {
            working_time_seconds: Some(secs.max(0.0).round() as u64),
            ..Default::default()
        });
    }

    /// 读取累计创作时间（秒）
    ///
    /// 未保存过（stats 缺失）时返回 0。
    pub fn working_time_seconds(&self) -> f64 {
        self.metadata
            .stats
            .as_ref()
            .and_then(|s| s.working_time_seconds)
            .unwrap_or(0) as f64
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
    fn test_working_time_seconds_roundtrip() {
        let mut project = LuminoProject::new("Test");
        // 未设置时返回 0
        assert_eq!(project.working_time_seconds(), 0.0);

        // 写入后读取（四舍五入取整）
        project.set_working_time_seconds(3661.7);
        assert_eq!(project.working_time_seconds(), 3662.0);

        // TOML 序列化包含 [stats] 段，反序列化保留
        let toml_str = project.metadata.to_toml_str().expect("序列化失败");
        assert!(
            toml_str.contains("[stats]"),
            "stats 段应被序列化: {toml_str}"
        );
        assert!(
            toml_str.contains("working_time_seconds = 3662"),
            "{toml_str}"
        );
        let decoded = ProjectMetadata::from_toml_str(&toml_str).expect("反序列化失败");
        assert_eq!(
            decoded.stats.expect("应有 stats 段").working_time_seconds,
            Some(3662)
        );

        // 负数钳制为 0
        project.set_working_time_seconds(-5.0);
        assert_eq!(project.working_time_seconds(), 0.0);
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

    #[test]
    fn test_apply_tempo_points() {
        let mut project = LuminoProject::new("Test");
        assert!(project.tempo_changes.is_empty());

        // 覆盖速度点：tick / BPM
        project.apply_tempo_points([(0.0, 140.0), (480.0, 90.5)]);
        assert_eq!(project.tempo_changes, vec![(0, 140.0), (480, 90.5)]);

        // 负 tick 收敛为 0，避免 u32 转换溢出
        project.apply_tempo_points([(-10.0, 100.0)]);
        assert_eq!(project.tempo_changes, vec![(0, 100.0)]);

        // 空列表清空速度变化
        project.apply_tempo_points(std::iter::empty());
        assert!(project.tempo_changes.is_empty());
    }
}
