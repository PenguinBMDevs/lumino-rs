//! 侧边栏核心结构 — `Sidebar` 主结构、构造与音轨管理方法
//!
//! 子模块组织（保持本文件 < 400 行）：
//! - `types`: 核心数据类型（常量、分组配置、音轨元数据、菜单状态）

use std::collections::HashSet;

use super::track_reorder::TrackReorderState;
pub use lumino_ui_core::sidebar_event::{GroupId, RollBarButton, Route};

mod types;
pub use types::{
    DEFAULT_PANEL_WIDTH, GroupSubState, MAX_PANEL_WIDTH, MIN_PANEL_WIDTH, MixerState,
    PanelContextMenuState, PendingTrackDeletionMeta, RESIZE_HANDLE_WIDTH, ROUTE_BAR_WIDTH, ROUTES,
    RouteConfig, StripParams, Track, TrackContextMenuState,
};

// ─── Sidebar 主结构 ───

/// 侧边栏主结构
pub struct Sidebar {
    /// 当前路由
    pub route: Route,
    pub(crate) panel_visible: bool,
    pub(crate) panel_route: Route,
    /// 音轨列表
    pub tracks: Vec<Track>,
    /// 当前选中的音轨索引
    pub selected_track: usize,
    /// 面板宽度（默认 200）
    pub panel_width: f32,
    /// 是否正在拖拽调整宽度
    pub(crate) is_resizing: bool,
    /// 拖拽开始时的鼠标 X 坐标
    pub(crate) resize_start_x: f32,
    /// 拖拽开始时的面板宽度
    pub(crate) resize_start_width: f32,
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
    /// 音轨选项卡右键菜单状态
    pub track_context_menu: TrackContextMenuState,
    /// 音轨列表面板空白区域右键菜单状态
    pub panel_context_menu: PanelContextMenuState,
    /// 正在重命名的音轨（音轨 ID，当前输入值）
    pub renaming_track: Option<(usize, String)>,
    /// 正在选择颜色的音轨 ID
    pub color_picking_track: Option<usize>,
    /// 单调递增的音轨 ID 计数器（删除后复用 ID 会导致选中冲突）
    pub(crate) next_track_id: usize,
    /// 已删除音轨的 ID 占用集合（新建音轨时跳过这些 ID）
    ///
    /// 用户需求：删除音轨后保留轨道编号为占用状态，新建音轨不能复用被删除音轨的编号。
    /// 永久销毁 `.lmdeltrack` 文件时从此集合移除对应 ID，随后才允许复用。
    pub(crate) reserved_track_ids: HashSet<usize>,
    /// 待 Root 消费的音轨删除请求（携带音轨 ID）
    ///
    /// Root 取出后转发给 Runner，由 Runner 将音轨数据写入 `.lmdeltrack` 缓存文件。
    /// 仅设置 ID——具体的音轨元数据（名称、port、channel、音符等）由 Root 从
    /// `editor_state.data` 中按 ID 查询得到。
    pub pending_track_deletion: Option<usize>,
    /// 待 Root 消费的音轨删除元数据缓存（与 pending_track_deletion 配对）
    ///
    /// 由于 `handle_track_context_menu_item_clicked` 在设置 pending_track_deletion
    /// 之前已从 `tracks` 中移除音轨入口，元数据无法事后查询。这里在移除前缓存一份，
    /// 供 Root 构造 `TrackDeletionPayload` 时使用。
    pub(crate) pending_track_deletion_meta: Option<PendingTrackDeletionMeta>,
    /// 待 Root 消费的"找回删除音轨"对话框打开请求
    ///
    /// Root 取出后转发给 Runner，由 Runner 调用 `DialogManager::open_recover_track`。
    pub pending_recover_track_dialog: bool,
    /// 音轨拖拽排序状态（None = 无拖拽进行中）
    pub track_reorder: Option<TrackReorderState>,
    /// 卷帘面板底部按钮当前激活项（`None` = 两个按钮均未点亮）
    ///
    /// 用单值 `Option` 而非两个 bool：横向/纵向三条杠的打开状态互斥，
    /// 互斥性由类型保证，无需运行时同步两个字段。
    ///
    /// 语义即「卷帘方向」：默认 `Some(Horizontal)` = 横向卷帘；
    /// `Some(Vertical)` = 纵向卷帘；`None` = 两个按钮均熄灭（理论不出现）。
    pub roll_bar_active: Option<RollBarButton>,
    /// 混音台状态（每条音轨的增益/声像，键为音轨 ID）
    pub mixer: MixerState,
}

impl Sidebar {
    /// 创建一个默认的侧边栏
    pub fn new() -> Self {
        Self {
            route: Route::File,
            panel_visible: true,
            panel_route: Route::File,
            tracks: vec![
                Track {
                    id: 0,
                    name: "Conductor".to_string(),
                    port: 0,
                    channel: 0,
                    display_label: "A01".to_string(),
                    is_conductor: true,
                    can_delete: false,
                    is_muted: false,
                    is_soloed: false,
                    color: None,
                },
                Track {
                    id: 1,
                    name: "Setup".to_string(),
                    port: 0,
                    channel: 0,
                    display_label: "A01".to_string(),
                    is_conductor: false,
                    can_delete: true,
                    is_muted: false,
                    is_soloed: false,
                    color: None,
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
            track_context_menu: TrackContextMenuState::default(),
            panel_context_menu: PanelContextMenuState::default(),
            renaming_track: None,
            color_picking_track: None,
            next_track_id: 2,
            reserved_track_ids: HashSet::new(),
            pending_track_deletion: None,
            pending_track_deletion_meta: None,
            pending_recover_track_dialog: false,
            track_reorder: None,
            // 默认进入横向卷帘（与用户「默认横向三条杠按钮」要求一致）
            roll_bar_active: Some(RollBarButton::Horizontal),
            mixer: MixerState::default(),
        }
    }

    /// 读取某音轨的混音台参数（增益/声像），缺失时返回默认。
    pub fn mixer_strip(&self, id: usize) -> StripParams {
        self.mixer.get(id)
    }

    /// 检查当前是否为音轨总览路由
    pub fn is_arrangement_route(&self) -> bool {
        self.route == Route::Arrangement
    }

    /// 当前是否处于钢琴卷帘面板（决定卷帘底部两个按钮是否显示）
    ///
    /// 与 `Root::right_sidebar_visible` 同源语义：卷帘 UI 可见，且主区域未被
    /// 工程走带 / 音频导出 / 视频导出面板替代。瀑布流模式下 `piano_roll_visible`
    /// 已被 `activate_group` 置为 false，因此本判定不涉及 AppMode。
    pub fn is_piano_roll_panel(&self) -> bool {
        self.piano_roll_visible
            && !self.is_arrangement_route()
            && !self.audio_export_visible
            && !self.video_export_visible
    }

    /// 当前是否处于纵向卷帘模式（卷帘内容沿时间轴竖直展开）
    ///
    /// 由底部「卷帘方向」按钮的激活项推导：仅 `Some(Vertical)` 为纵向。
    pub fn is_vertical_roll(&self) -> bool {
        self.roll_bar_active == Some(RollBarButton::Vertical)
    }

    /// yinhe 风格端口字母：port 0→'A', 1→'B', ..., 25→'Z'，超限为 '?'
    fn port_letter(port: u8) -> char {
        if port < 26 {
            (b'A' + port) as char
        } else {
            '?'
        }
    }

    /// yinhe 风格音轨标签：`{端口字母}{通道号+1:02}`
    /// 标签是纯通道指示器，同一端口/通道的多个音轨共享相同标签。
    pub fn track_label(port: u8, channel: u8) -> String {
        format!("{}{:02}", Self::port_letter(port), channel + 1)
    }

    /// 从 MIDI 数据更新音轨列表（按 port→channel→id 排序，同端口按通道号排列）
    /// 排序键：port（端口字母 A→Z），channel（通道号 01→16），id（稳定排序保序）
    /// track_infos: (track_index, track_name, note_count, channel, port)
    pub fn update_tracks_from_midi(
        &mut self,
        track_infos: &[(usize, Option<String>, u64, u8, u8)],
    ) {
        tracing::info!("update_tracks_from_midi: {} tracks", track_infos.len());
        self.tracks.clear();
        // 加载新工程时清空当前 session 的已删除音轨 ID 占用集合，
        // 避免旧 session 的 reserved ID 影响新工程的音轨 ID 分配。
        self.reserved_track_ids.clear();
        self.pending_track_deletion = None;
        self.pending_track_deletion_meta = None;
        self.pending_recover_track_dialog = false;

        for (idx, (track_idx, name, _note_count, ch, port)) in track_infos.iter().enumerate() {
            let track_name = name.as_deref().unwrap_or("Unknown");
            let label = Self::track_label(*port, *ch);
            tracing::debug!(
                "  track {}: id={}, name={}, port={}, channel={}, label={}",
                idx,
                track_idx,
                track_name,
                port,
                ch,
                label
            );
            self.tracks.push(Track {
                id: *track_idx,
                name: track_name.to_string(),
                port: *port,
                channel: *ch,
                display_label: label,
                is_conductor: *track_idx == 0,
                can_delete: *track_idx != 0,
                is_muted: false,
                is_soloed: false,
                color: None,
            });
        }

        // 按 port→channel→id 排序：同一端口的音轨按通道号排列
        // 端口 A→Z，通道 01→16，id 保序
        self.tracks.sort_by_key(|t| (t.port, t.channel, t.id));

        // 如果有音轨，默认选择第一个
        if !self.tracks.is_empty() {
            self.selected_track = self.tracks[0].id;
            tracing::info!("default selected_track = {}", self.selected_track);
        }

        // 同步 next_track_id
        let max_id = self.tracks.iter().map(|track| track.id).max().unwrap_or(0);
        self.next_track_id = self.next_track_id.max(max_id + 1);
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
    /// 取出并清空待 Root 消费的音轨删除请求
    pub fn take_pending_track_deletion(&mut self) -> Option<usize> {
        self.pending_track_deletion.take()
    }

    /// 取出并清空待 Root 消费的音轨删除元数据缓存
    pub fn take_pending_track_deletion_meta(&mut self) -> Option<PendingTrackDeletionMeta> {
        self.pending_track_deletion_meta.take()
    }

    /// 取出并清空"找回删除音轨"对话框打开请求
    pub fn take_pending_recover_track_dialog(&mut self) -> bool {
        let v = self.pending_recover_track_dialog;
        self.pending_recover_track_dialog = false;
        v
    }

    /// 设置面板右键菜单位置（由 Host 在 process_message 中捕获鼠标位置后调用）
    pub fn set_panel_context_menu_pos(&mut self, x: f32, y: f32) {
        self.panel_context_menu.mouse_pos = Some((x, y));
    }

    /// 分配新的音轨 ID，跳过 `reserved_track_ids` 中已占用的 ID
    ///
    /// 用户需求：删除音轨后保留轨道编号为占用状态，新建音轨不能复用被删除音轨的编号。
    /// 永久销毁 `.lmdeltrack` 文件时通过 `release_reserved_track_id` 释放对应 ID。
    pub(crate) fn allocate_track_id(&mut self) -> usize {
        while self.reserved_track_ids.contains(&self.next_track_id) {
            self.next_track_id += 1;
        }
        let new_id = self.next_track_id;
        self.next_track_id += 1;
        new_id
    }

    /// 永久释放已删除音轨的 ID 占用（销毁 `.lmdeltrack` 后调用）
    ///
    /// 释放后，该 ID 可被新建音轨复用。本方法用于"永久删除"路径；
    /// "恢复"路径不释放 ID（因为音轨重新出现在 sidebar.tracks 中）。
    pub fn release_reserved_track_id(&mut self, id: usize) {
        self.reserved_track_ids.remove(&id);
    }

    /// 标记指定音轨 ID 为已删除占用状态
    pub(crate) fn mark_track_id_reserved(&mut self, id: usize) {
        self.reserved_track_ids.insert(id);
    }

    /// 检查指定 ID 是否为已删除占用的轨道编号
    ///
    /// 预留 API：供外部模块（如协作同步、工程导出）查询轨道 ID 占用状态。
    #[allow(dead_code)]
    pub fn is_track_id_reserved(&self, id: usize) -> bool {
        self.reserved_track_ids.contains(&id)
    }
}

impl Default for Sidebar {
    fn default() -> Self {
        Self::new()
    }
}
