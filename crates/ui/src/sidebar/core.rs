use crate::resources::icon;
use lumino_core::storage::config::TrackDisplayMode;

pub use lumino_ui_core::sidebar_event::{GroupId, Route};

/// 路由栏宽度（固定）
pub const ROUTE_BAR_WIDTH: f32 = 48.0;
/// 面板默认宽度
pub const DEFAULT_PANEL_WIDTH: f32 = 200.0;
/// 面板最小宽度
pub const MIN_PANEL_WIDTH: f32 = 150.0;
/// 面板最大宽度
pub const MAX_PANEL_WIDTH: f32 = 400.0;
/// 调整大小手柄宽度
pub const RESIZE_HANDLE_WIDTH: f32 = 6.0;

// ─── 分组系统 ───

/// 分组子按钮状态（切换分组时保存/恢复）
#[derive(Debug, Clone)]
pub struct GroupSubState {
    pub panel_visible: bool,
    pub panel_route: Route,
    pub automation_panel_visible: bool,
}

impl Default for GroupSubState {
    fn default() -> Self {
        Self {
            panel_visible: false,
            panel_route: Route::File,
            automation_panel_visible: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum RouteConfig {
    /// 组父按钮（定义分组，带颜色指示）
    GroupParent {
        group: GroupId,
        icon: icon::Icon,
    },
    /// 路由项（可关联到某个组作为子按钮）
    Item {
        route: Route,
        icon: icon::Icon,
        group: Option<GroupId>,
    },
    Space,
}

pub const ROUTES: [RouteConfig; 9] = [
    // ── 钢琴卷帘组（红色） ──
    RouteConfig::GroupParent {
        group: GroupId::PianoRoll,
        icon: icon::Keys,
    },
    RouteConfig::Item {
        route: Route::File,
        icon: icon::FolderTree,
        group: Some(GroupId::PianoRoll),
    },
    RouteConfig::Item {
        route: Route::Automation,
        icon: icon::WaveForm,
        group: Some(GroupId::PianoRoll),
    },
    // ── 工程走带组（绿色） ──
    RouteConfig::GroupParent {
        group: GroupId::Project,
        icon: icon::Arrangement,
    },
    // ── 瀑布流播放器组（黄色） ──
    RouteConfig::GroupParent {
        group: GroupId::Waterfall,
        icon: icon::PlayCircle,
    },
    // ── 渲染组（蓝色） ──
    RouteConfig::GroupParent {
        group: GroupId::Renderer,
        icon: icon::Download,
    },
    RouteConfig::Item {
        route: Route::VideoExport,
        icon: icon::VideoCamera,
        group: Some(GroupId::Renderer),
    },
    RouteConfig::Item {
        route: Route::AudioExport,
        icon: icon::MusicNote,
        group: Some(GroupId::Renderer),
    },
    // ── 弹性空间 ──
    RouteConfig::Space,
];

// ─── 音轨数据 ───

#[derive(Debug, Clone)]
pub struct Track {
    pub id: usize,
    pub name: String,
    pub channel: u8,
    pub display_label: String,
    pub is_conductor: bool,
    pub can_delete: bool,
    pub is_muted: bool,
}

// ─── Sidebar 主结构 ───

pub struct Sidebar {
    pub route: Route,
    pub(crate) panel_visible: bool,
    pub(crate) panel_route: Route,
    pub tracks: Vec<Track>,
    pub selected_track: usize,
    /// 面板宽度（默认 200）
    pub panel_width: f32,
    /// 是否正在拖拽调整宽度
    pub(crate) is_resizing: bool,
    /// 拖拽开始时的鼠标 X 坐标
    pub(crate) resize_start_x: f32,
    /// 拖拽开始时的面板宽度
    pub(crate) resize_start_width: f32,
    /// 音轨列表显示模式
    pub track_display_mode: TrackDisplayMode,
    /// 自动化面板是否可见（独立于路由面板）
    pub automation_panel_visible: bool,
    /// 钢琴卷帘编辑器是否可见（默认打开）
    pub piano_roll_visible: bool,
    // ── 分组状态 ──
    /// 当前激活的分组（None = 无分组激活）
    pub active_group: Option<GroupId>,
    /// 钢琴卷帘组的子按钮保存状态
    pub piano_roll_sub_state: GroupSubState,
    /// 工程走带组的子按钮保存状态
    pub project_sub_state: GroupSubState,
    /// 渲染组的子按钮保存状态
    pub renderer_sub_state: GroupSubState,
    /// 音频渲染面板是否可见（在主界面钢琴卷帘区域显示）
    pub audio_export_visible: bool,
    /// 视频渲染面板是否可见（在主界面钢琴卷帘区域显示）
    pub video_export_visible: bool,
}

impl Sidebar {
    pub fn new() -> Self {
        Self {
            route: Route::File,
            panel_visible: true,
            panel_route: Route::File,
            tracks: vec![
                Track {
                    id: 0,
                    name: "Conductor".to_string(),
                    channel: 0,
                    display_label: "A01".to_string(),
                    is_conductor: true,
                    can_delete: false,
                    is_muted: false,
                },
                Track {
                    id: 1,
                    name: "Setup".to_string(),
                    channel: 0,
                    display_label: "A02".to_string(),
                    is_conductor: false,
                    can_delete: true,
                    is_muted: false,
                },
            ],
            selected_track: 0,
            panel_width: DEFAULT_PANEL_WIDTH,
            is_resizing: false,
            resize_start_x: 0.0,
            resize_start_width: DEFAULT_PANEL_WIDTH,
            automation_panel_visible: false,
            piano_roll_visible: true,
            active_group: Some(GroupId::PianoRoll),
            piano_roll_sub_state: GroupSubState::default(),
            project_sub_state: GroupSubState::default(),
            renderer_sub_state: GroupSubState::default(),
            audio_export_visible: false,
            video_export_visible: false,
            track_display_mode: TrackDisplayMode::default(),
        }
    }

    /// 检查当前是否为音轨总览路由
    pub fn is_arrangement_route(&self) -> bool {
        self.route == Route::Arrangement
    }

    /// 从 MIDI 数据更新音轨列表
    pub fn update_tracks_from_midi(&mut self, track_infos: &[(usize, Option<String>, u64, u8)]) {
        tracing::info!("update_tracks_from_midi: {} tracks", track_infos.len());
        self.tracks.clear();

        // 先收集所有音轨的 channel 信息
        let channels: Vec<u8> = track_infos.iter().map(|(_, _, _, ch)| *ch).collect();

        // 根据模式计算显示标签
        match self.track_display_mode {
            TrackDisplayMode::ChannelGrouped => {
                // 按通道分组，组内按 track_idx 排序
                let mut grouped: Vec<Vec<usize>> = vec![Vec::new(); 16];
                for (idx, (_, _, _, _)) in track_infos.iter().enumerate() {
                    grouped[channels[idx] as usize].push(idx);
                }

                for group in grouped {
                    let mut sorted_group = group;
                    sorted_group.sort_by_key(|&i| {
                        track_infos[i].0 // track_idx
                    });
                    for (label_idx, &idx) in sorted_group.iter().enumerate() {
                        let (track_idx, name, _note_count, ch) = &track_infos[idx];
                        let track_name = name.as_deref().unwrap_or("Unknown");
                        let channel_letter = (b'A' + channels[idx]) as char;
                        let label = format!("{}{:02}", channel_letter, label_idx + 1);
                        tracing::debug!(
                            "  track {}: id={}, name={}, channel={}, label={}",
                            idx,
                            track_idx,
                            track_name,
                            ch,
                            label
                        );
                        self.tracks.push(Track {
                            id: *track_idx,
                            name: label.clone(),
                            channel: channels[idx],
                            display_label: label,
                            is_conductor: *track_idx == 0,
                            can_delete: *track_idx != 0,
                            is_muted: false,
                        });
                    }
                }
            }
            TrackDisplayMode::TrackIndex => {
                for (idx, (track_idx, name, _note_count, ch)) in track_infos.iter().enumerate() {
                    let track_name = name.as_deref().unwrap_or("Unknown");
                    let label = format!("{:02}", idx + 1);
                    tracing::debug!(
                        "  track {}: id={}, name={}, channel={}, label={}",
                        idx,
                        track_idx,
                        track_name,
                        ch,
                        label
                    );
                    self.tracks.push(Track {
                        id: *track_idx,
                        name: label.clone(),
                        channel: *ch,
                        display_label: label,
                        is_conductor: *track_idx == 0,
                        can_delete: *track_idx != 0,
                        is_muted: false,
                    });
                }
            }
        }

        // 如果有音轨，默认选择第一个
        if !self.tracks.is_empty() {
            self.selected_track = self.tracks[0].id;
            tracing::info!("default selected_track = {}", self.selected_track);
        }
    }

    /// 重新应用音轨列表显示模式（设置变更时调用）
    /// 根据当前 `track_display_mode` 重新排列和标记音轨。
    pub fn reapply_display_mode(&mut self) {
        if self.tracks.is_empty() {
            return;
        }

        // 保存原始数据用于重新计算
        let old_tracks: Vec<Track> = self.tracks.clone();

        // 保留当前选中音轨
        let current_selected = self.selected_track;

        self.tracks.clear();

        match self.track_display_mode {
            TrackDisplayMode::ChannelGrouped => {
                // 按通道分组，组内按 id 排序
                let mut grouped: Vec<Vec<usize>> = vec![Vec::new(); 16];
                for (idx, t) in old_tracks.iter().enumerate() {
                    grouped[t.channel as usize].push(idx);
                }

                for group in grouped {
                    let mut sorted_group = group;
                    // 默认 conductor 优先，其余按 id 排序
                    sorted_group
                        .sort_by_key(|&i| (old_tracks[i].is_conductor as u8, old_tracks[i].id));
                    for (label_idx, &idx) in sorted_group.iter().enumerate() {
                        let t = &old_tracks[idx];
                        let channel_letter = (b'A' + t.channel) as char;
                        let label = format!("{}{:02}", channel_letter, label_idx + 1);
                        self.tracks.push(Track {
                            id: t.id,
                            name: label.clone(),
                            channel: t.channel,
                            display_label: label,
                            is_conductor: t.is_conductor,
                            can_delete: t.can_delete,
                            is_muted: t.is_muted,
                        });
                    }
                }
            }
            TrackDisplayMode::TrackIndex => {
                // 按 id 排序，标记为 01, 02, 03...
                let mut sorted: Vec<&Track> = old_tracks.iter().collect();
                sorted.sort_by_key(|t| t.id);
                for (idx, t) in sorted.iter().enumerate() {
                    let label = format!("{:02}", idx + 1);
                    self.tracks.push(Track {
                        id: t.id,
                        name: label.clone(),
                        channel: t.channel,
                        display_label: label,
                        is_conductor: t.is_conductor,
                        can_delete: t.can_delete,
                        is_muted: t.is_muted,
                    });
                }
            }
        }

        // 恢复选中音轨（如果还在的话）
        if self.tracks.iter().any(|t| t.id == current_selected) {
            self.selected_track = current_selected;
        } else if !self.tracks.is_empty() {
            self.selected_track = self.tracks[0].id;
        }

        tracing::info!(
            "reapply_display_mode: mode={:?}, tracks={}",
            self.track_display_mode,
            self.tracks.len()
        );
    }

    /// 设置当前选中的音轨（默认强制打开面板，供测试使用）
    #[cfg(test)]
    pub fn set_selected_track(&mut self, track_idx: usize) {
        self.set_selected_track_with_panel(track_idx, true);
    }

    /// 设置当前选中的音轨（可控制是否强制打开面板）
    ///
    /// `open_panel` 为 `true` 时，在非 Arrangement 模式下强制打开面板（用户手动选轨）；
    /// 为 `false` 时只刷新数据，不改变面板可见性（MIDI 加载等程序化操作）。
    pub fn set_selected_track_with_panel(&mut self, track_idx: usize, open_panel: bool) {
        self.selected_track = track_idx;
        if open_panel && self.route != Route::Arrangement {
            self.panel_visible = true;
        }
    }
}

impl Default for Sidebar {
    fn default() -> Self {
        Self::new()
    }
}
