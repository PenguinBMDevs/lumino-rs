//! 生产状态 → egui 外壳视图模型。
//!
//! 只做数据搬运，不含任何渲染逻辑；渲染在 `lumino_ui_egui::chrome::show_bands`。
//! 这样叠加层与离线场景共用同一份绘制代码，也避免 `lumino-ui-egui` 反向依赖 iced。

use lumino_core::storage::config::NoteCountDisplay;
use lumino_extras::i18n::main_translations;
use lumino_ui_egui::chrome::{
    ChromeModel, RollBar, SidebarGroup, SidebarModel, SidebarPanel, SidebarTrack, StatusbarModel,
    TitlebarModel, ToolbarModel,
};

use crate::root::Root;
use crate::sidebar::{RollBarButton, Route, Sidebar};
use crate::toolbar::ToolbarPerfContext;

/// iced 分组 → egui 分组。
fn sidebar_group(group: crate::sidebar::GroupId) -> SidebarGroup {
    match group {
        crate::sidebar::GroupId::PianoRoll => SidebarGroup::PianoRoll,
        crate::sidebar::GroupId::Project => SidebarGroup::Project,
        crate::sidebar::GroupId::Waterfall => SidebarGroup::Waterfall,
        crate::sidebar::GroupId::Renderer => SidebarGroup::Renderer,
    }
}

/// iced 颜色（线性浮点）→ egui 颜色。
fn color32(color: iced_core::Color) -> egui::Color32 {
    let to_u8 = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    egui::Color32::from_rgb(to_u8(color.r), to_u8(color.g), to_u8(color.b))
}

impl Sidebar {
    /// 是否存在必须由 iced 独占绘制的瞬态 UI。
    ///
    /// 重命名输入框、拖拽插入线、上下文菜单、颜色/端口/通道选择器、宽度拖拽
    /// 都依赖 iced 自己的命中与焦点管理；这些状态下叠加层不得接管左侧栏，
    /// 否则会出现「egui 盖住 iced 输入框，输入却进 iced」的错位。
    fn has_transient_ui(&self) -> bool {
        self.renaming_track.is_some()
            || self
                .track_reorder
                .as_ref()
                .is_some_and(|state| state.active)
            || self.track_context_menu.target_track_id.is_some()
            || self.panel_context_menu.is_open
            || self.color_picking_track.is_some()
            || self.port_picking_track.is_some()
            || self.channel_picking_track.is_some()
            || self.is_resizing
    }

    /// 构造左侧栏只读模型；存在瞬态 UI 时返回 `None`，交给 iced 继续绘制。
    fn egui_model(
        &self,
        language: lumino_extras::i18n::Language,
        mixer_open: bool,
    ) -> Option<SidebarModel> {
        if self.has_transient_ui() {
            return None;
        }
        let translations = main_translations(language);

        let panel = match self.panel_route {
            Route::File => SidebarPanel::Tracks,
            Route::Arrangement => SidebarPanel::AddTrackOnly,
            _ => SidebarPanel::Blank,
        };

        // 生产 `route.rs` 的 `Route::File` 子按钮规则：
        // `active == Arrangement` 时不点亮，否则 `panel_visible && route == File`。
        let file_active =
            self.route != Route::Arrangement && self.panel_visible && self.route == Route::File;

        let tracks = self
            .tracks
            .iter()
            .map(|track| SidebarTrack {
                label: track.display_label.clone(),
                name: track.name.clone(),
                color: track.color.map(color32),
                selected: track.id == self.selected_track,
                conductor: track.is_conductor,
                soloed: track.is_soloed,
            })
            .collect();

        Some(SidebarModel {
            panel_visible: self.panel_visible,
            panel_width: self.panel_width,
            piano_roll_visible: self.piano_roll_visible,
            waterfall: false,
            active_group: self.active_group.map(sidebar_group),
            automation_panel_visible: self.automation_panel_visible,
            audio_export_visible: self.audio_export_visible,
            video_export_visible: self.video_export_visible,
            file_active,
            roll_bar_visible: false,
            roll_bar_active: match self.roll_bar_active {
                Some(RollBarButton::Horizontal) => Some(RollBar::Horizontal),
                Some(RollBarButton::Vertical) => Some(RollBar::Vertical),
                None => None,
            },
            mixer_open,
            panel,
            tracks,
            track_list_title: translations.sidebar_track_list.to_owned(),
            add_track_label: translations.sidebar_add_track.to_owned(),
        })
    }
}

impl Root {
    /// 构造 egui 外壳快照（标题栏 + 工具栏 + 左侧栏 + 状态栏，只读）。
    pub(crate) fn egui_chrome_model(&self) -> ChromeModel {
        let language = self.settings.display.language;
        let translations = main_translations(language);
        let waterfall = self.state.current_mode == crate::titlebar::mode_toggle::AppMode::Waterfall;

        // 系统原生标题栏模式下，顶部 30px 由操作系统绘制，egui 不接管。
        let titlebar = (!self.settings.synth.use_native_titlebar).then(|| TitlebarModel {
            focused: self.window.is_focused,
            menus: vec![
                translations.menu_file.to_owned(),
                translations.menu_edit.to_owned(),
                translations.menu_view.to_owned(),
                translations.menu_help.to_owned(),
            ],
        });

        // 工具栏：与 iced 仪表盘共用同一套 tick→BPM/时间换算。
        // 仅在实际渲染工具栏的视图里接管（瀑布流 / 导出面板 / 渲染器面板没有工具栏）。
        let perf = ToolbarPerfContext {
            playback_tick: self.editor.playback_position,
            ppq: self.editor.editor_state.view.ppq,
            tempo_points: &self.editor.editor_state.data.tempo_points,
        };
        let toolbar = self.toolbar.egui_snapshot(&perf);
        // 工具栏与左侧栏共用同一份浮点宽度：Sidebar::width() 会截断到整数，
        // 拖拽调整面板宽度时会在两条 band 之间留下 0~1px 缝隙。
        let sidebar_width = 48.0
            + if self.sidebar.panel_visible {
                self.sidebar.panel_width
            } else {
                0.0
            };
        let shows_toolbar = !waterfall
            && (self.sidebar.is_arrangement_route()
                || (!self.sidebar.audio_export_visible
                    && !self.sidebar.video_export_visible
                    && self.sidebar.piano_roll_visible));

        let note_count = match self.settings.display.note_count_display {
            NoteCountDisplay::StatusBar => Some(self.editor_note_count()),
            NoteCountDisplay::ProjectSettings => None,
        };
        let status = self.statusbar.egui_snapshot(language, note_count);
        // 生产 `perf_section` 仅在 `fps` 已采样时渲染；同组隐藏 CPU 数值。
        let metrics_visible = status.fps.is_some();

        // 左侧栏：瀑布流模式同样存在（`view_waterfall_fullscreen` 也渲染左栏）。
        let mut sidebar = self.sidebar.egui_model(language, self.mixer_panel.open);
        if let Some(model) = sidebar.as_mut() {
            model.waterfall = waterfall;
            model.roll_bar_visible = !waterfall && self.sidebar.is_piano_roll_panel();
        }

        // 工具栏模型：只在 iced 实际渲染工具栏的视图里接管。
        let toolbar = if shows_toolbar {
            Some(ToolbarModel {
                left_inset: sidebar_width,
                bpm: toolbar.bpm,
                ppq: toolbar.ppq,
                position: toolbar.position,
                time: toolbar.time,
                snap: toolbar.snap,
                tool: toolbar.tool,
                draw_panel_open: toolbar.draw_panel_open,
                playing: toolbar.playing,
                looping: toolbar.looping,
                recording: toolbar.recording,
                onion: toolbar.onion,
                auto_scroll: toolbar.auto_scroll,
            })
        } else {
            None
        };

        ChromeModel {
            titlebar,
            toolbar,
            sidebar,
            statusbar: StatusbarModel {
                left: status.left_text,
                left_accent: status.left_accent,
                cpu_percent: metrics_visible.then_some(status.cpu_percent),
                memory_mb: status.memory_mb,
                fps: status.fps,
                right: status.right_text,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_extras::i18n::Language;

    fn sidebar_model(sidebar: &Sidebar) -> Option<SidebarModel> {
        sidebar.egui_model(Language::ZhCn, false)
    }

    #[test]
    fn sidebar_model_describes_default_panel() {
        let model = sidebar_model(&Sidebar::new()).expect("默认状态应可接管");
        assert!(model.panel_visible);
        assert_eq!(model.panel, SidebarPanel::Tracks);
        assert!(model.file_active);
        assert_eq!(model.tracks.len(), 2);
        assert!(model.tracks[0].conductor);
        assert!(model.tracks[0].selected);
        assert_eq!(model.tracks[1].label, "A01");
        assert_eq!(model.tracks[1].name, "Setup");
        assert!((model.total_width() - 248.0).abs() < f32::EPSILON);
        assert!(!model.track_list_title.is_empty());
        assert!(!model.add_track_label.is_empty());
    }

    #[test]
    fn sidebar_model_hides_panel_without_disabling_takeover() {
        let mut sidebar = Sidebar::new();
        sidebar.panel_visible = false;
        let model = sidebar_model(&sidebar).expect("隐藏面板不是瞬态 UI");
        assert!(!model.panel_visible);
        assert!(!model.file_active);
        assert!((model.total_width() - 48.0).abs() < f32::EPSILON);
    }

    #[test]
    fn sidebar_model_defers_to_iced_while_transient() {
        let mut renaming = Sidebar::new();
        renaming.renaming_track = Some((1, "草稿".to_owned()));
        assert!(sidebar_model(&renaming).is_none(), "重命名中不得接管");

        let mut menu = Sidebar::new();
        menu.track_context_menu.target_track_id = Some(1);
        assert!(sidebar_model(&menu).is_none(), "音轨菜单打开时不得接管");

        let mut menu = Sidebar::new();
        menu.panel_context_menu.is_open = true;
        assert!(sidebar_model(&menu).is_none(), "面板菜单打开时不得接管");

        let mut picking = Sidebar::new();
        picking.color_picking_track = Some(1);
        assert!(sidebar_model(&picking).is_none(), "取色中不得接管");

        let mut picking = Sidebar::new();
        picking.port_picking_track = Some(1);
        assert!(sidebar_model(&picking).is_none(), "端口选择中不得接管");

        let mut picking = Sidebar::new();
        picking.channel_picking_track = Some(1);
        assert!(sidebar_model(&picking).is_none(), "通道选择中不得接管");

        let mut resizing = Sidebar::new();
        resizing.is_resizing = true;
        assert!(sidebar_model(&resizing).is_none(), "拖拽宽度时不得接管");
    }

    #[test]
    fn sidebar_model_allows_candidate_reorder() {
        // 仅按下（候选）未激活时，插入指示线尚未出现，可以继续由 egui 接管。
        let mut sidebar = Sidebar::new();
        sidebar.track_reorder = Some(crate::sidebar::TrackReorderState {
            track_id: 1,
            started_at: std::time::Instant::now(),
            press_pos: iced_core::Point::new(0.0, 0.0),
            active: false,
            hover_index: None,
        });
        assert!(sidebar_model(&sidebar).is_some());

        if let Some(state) = sidebar.track_reorder.as_mut() {
            state.active = true;
            state.hover_index = Some(1);
        }
        assert!(sidebar_model(&sidebar).is_none(), "拖拽激活时不得接管");
    }
}
