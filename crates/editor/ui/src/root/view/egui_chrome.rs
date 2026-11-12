//! 生产状态 → egui 外壳视图模型。
//!
//! 只做数据搬运，不含任何渲染逻辑；渲染在 `lumino_ui_egui::chrome::show_bands`。
//! 这样叠加层与离线场景共用同一份绘制代码，也避免 `lumino-ui-egui` 反向依赖 iced。

mod arrangement;
mod audio_export_panel;
mod bend_buttons;
mod context_menu;
mod curve_preview;
mod dialogs;
mod editor_buttons;
mod egg;
mod image_to_midi;
mod marquee;
mod mask_panel;
mod material_delete;
mod menu;
mod mixer;
mod palette;
mod renderer_panel;
mod right_sidebar;
mod settings_audio;
mod settings_pages;
mod settings_ui;
mod shape_canvas;
mod shape_drawer;
mod shape_preview;
mod sidebar_panel;
mod text_tool;
mod titlebar;
mod toast;
mod tool_panel;
mod toolbar;
mod velocity_panel;
mod video_clip_timeline;
mod video_export;
mod video_preview;

use lumino_core::storage::config::NoteCountDisplay;
use lumino_extras::i18n::main_translations;
use lumino_ui_egui::chrome::piano_roll::PianoRollView;
use lumino_ui_egui::chrome::{
    ChromeModel, PopupTitlebarModel, RollBar, SidebarGroup, SidebarModel, SidebarPanel,
    SidebarTrack, StatusbarModel, TitlebarModel, ToolbarModel,
};
use lumino_ui_egui::chrome::{SidebarRouteIcon, SidebarRouteItem};

use crate::root::Root;
use crate::sidebar::{RollBarButton, Route, Sidebar};
use crate::toolbar::ToolbarPerfContext;
use lumino_ui_egui::layout;

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
pub(super) fn color32(color: iced_core::Color) -> egui::Color32 {
    let to_u8 = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    egui::Color32::from_rgb(to_u8(color.r), to_u8(color.g), to_u8(color.b))
}

impl Sidebar {
    /// 是否存在必须由 iced 独占绘制的瞬态 UI。
    ///
    /// 重命名输入框、拖拽插入线、颜色/端口/通道选择器、宽度拖拽都依赖 iced 自己的
    /// 命中与焦点管理；这些状态下叠加层不得接管左侧栏，否则会出现「egui 盖住 iced
    /// 输入框，输入却进 iced」的错位。
    ///
    /// 注意：音轨/面板右键菜单与**行内重命名**都已由 egui 绘制并接管
    /// （`chrome/sidebar/menu.rs`、`track_list.rs`）；若仍在此返回 `None`，
    /// 底层画布纹理会盖住 iced 的绘制，真机表现为「左栏整块空白」。
    fn has_transient_ui(&self) -> bool {
        // 只剩拖拽排序与宽度拖拽：取色器 / 端口 / 通道选择器、右键菜单、行内重命名
        // 均已由 egui 绘制（返回 None 会让底层画布纹理盖住 iced 绘制 → 左栏空白）。
        // 已无瞬态 UI：拖拽排序、菜单、重命名、取色/端口/通道、宽度拖拽
        // 全部由 egui 绘制并接管。
        false
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
                id: track.id,
                label: track.display_label.clone(),
                name: track.name.clone(),
                color: track.color.map(color32),
                selected: track.id == self.selected_track,
                conductor: track.is_conductor,
                soloed: track.is_soloed,
                renaming: self
                    .renaming_track
                    .as_ref()
                    .is_some_and(|(id, _)| *id == track.id),
                rename_buffer: self
                    .renaming_track
                    .as_ref()
                    .filter(|(id, _)| *id == track.id)
                    .map(|(_, buf)| buf.clone())
                    .unwrap_or_default(),
            })
            .collect();

        // 路由栏条目：与 `sidebar/route.rs`（iced）同一套可见性 / 点亮规则，
        // 顺序即 `ROUTES` 顺序。两边同源，绘制与命中不会再错位。
        let active_group = self.active_group.map(sidebar_group);
        let mut route_items = vec![SidebarRouteItem {
            icon: SidebarRouteIcon::PianoRoll,
            active: self.piano_roll_visible,
        }];
        if active_group == Some(SidebarGroup::PianoRoll) {
            route_items.push(SidebarRouteItem {
                icon: SidebarRouteIcon::File,
                active: file_active,
            });
            route_items.push(SidebarRouteItem {
                icon: SidebarRouteIcon::Automation,
                active: self.automation_panel_visible,
            });
        }
        route_items.push(SidebarRouteItem {
            icon: SidebarRouteIcon::Project,
            active: active_group == Some(SidebarGroup::Project),
        });
        route_items.push(SidebarRouteItem {
            icon: SidebarRouteIcon::Waterfall,
            active: false,
        });
        route_items.push(SidebarRouteItem {
            icon: SidebarRouteIcon::Renderer,
            active: active_group == Some(SidebarGroup::Renderer),
        });
        if active_group == Some(SidebarGroup::Renderer) {
            route_items.push(SidebarRouteItem {
                icon: SidebarRouteIcon::VideoExport,
                active: self.video_export_visible,
            });
            route_items.push(SidebarRouteItem {
                icon: SidebarRouteIcon::AudioExport,
                active: self.audio_export_visible,
            });
        }

        Some(SidebarModel {
            panel_visible: self.panel_visible,
            panel_width: self.panel_width,
            piano_roll_visible: self.piano_roll_visible,
            waterfall: false,
            active_group,
            route_items,
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
            context_menu: sidebar_panel::track_menu(self),
            panel_menu: sidebar_panel::panel_menu(self),
            color_picker: sidebar_panel::color_picker(self),
            numeric_picker: sidebar_panel::numeric_picker(self),
            resizing: self.is_resizing,
            reorder: self
                .track_reorder
                .as_ref()
                .filter(|state| state.active)
                .map(|state| lumino_ui_egui::chrome::SidebarReorderModel {
                    track_id: state.track_id,
                    hover_index: state.hover_index,
                }),
        })
    }
}

impl Root {
    /// 卷帘骨架是否由 egui 接管（gfx 只保留网格线与音符）。
    ///
    /// 必须与模型装配同源：模型为空（循环区间 / 中途变拍号 / 洋葱皮 / 协作光标 /
    /// 非卷帘视图 …）时 gfx 仍要绘制键盘与标尺，否则整块骨架会消失。
    pub(crate) fn egui_piano_roll_active(&self) -> bool {
        crate::host::overlay_enabled() && self.egui_piano_roll_model().is_some()
    }

    /// 卷帘骨架模型：只在 iced 用纯 UI 绘制的场合接管。
    ///
    /// 以下情形必须让位给 iced（否则会盖掉动态内容）：
    /// - 纵向卷帘（布局不同）；
    /// - 循环区间开启（标尺里画循环带与把手）；
    /// - 拍号在工程中途变化（标尺分段）；
    /// - 洋葱皮开启（琴键带彩色覆盖层）；
    /// - 协作远程光标（光标可落在键盘/标尺上）；
    /// - 当前不是钢琴卷帘视图（走带 / 导出面板 / 卷帘关闭）。
    fn egui_piano_roll_model(&self) -> Option<PianoRollView> {
        let left_inset = 48.0
            + if self.sidebar.panel_visible {
                self.sidebar.panel_width
            } else {
                0.0
            };
        let right_inset = if self.right_sidebar_visible() {
            48.0 + if self.right_sidebar.panel_visible {
                self.right_sidebar.panel_width
            } else {
                0.0
            }
        } else {
            0.0
        };
        if self.sidebar.is_vertical_roll()
            || !self.right_sidebar_visible()
            || self.toolbar.onion_skin_enabled
            || !self.editor.remote_cursors.is_empty()
            || self
                .editor
                .loop_range
                .as_ref()
                .is_some_and(|range| range.enabled())
        {
            return None;
        }
        let signatures = self.editor.editor_state.data.time_signatures.as_slice();
        if signatures.len() > 1 {
            return None;
        }

        let view = &self.editor.editor_state.view;
        let canvas = self.editor.editor_state.canvas;
        if canvas.size_x <= 0.0 || canvas.size_y <= 0.0 {
            return None;
        }

        // 拍号 → 每小节 tick（默认 4/4）。
        let (numerator, denominator) = signatures.first().map_or((4u8, 4u8), |&(_, n, d)| (n, d));
        let ppq = view.ppq;
        let ticks_per_measure =
            f32::from(ppq) * 4.0 * f32::from(numerator) / f32::from(denominator.max(1));

        // 滚动条按 iced `ScrollbarWidget::thumb_geometry` 的口径换算成比例：
        // 水平 viewport 缺省为轨道长度；垂直 viewport 为画布高 - 标尺高。
        let (max_x, max_y) = self.editor.editor_state.max_scroll;
        let track_x = (canvas.size_x + 16.0).max(1.0);
        let track_y = (canvas.size_y - 4.0).max(1.0);
        let viewport_y = (canvas.size_y - view.ruler_height).max(0.0);
        let scrollable_x = (max_x - track_x).max(0.0);
        let scrollable_y = (max_y - viewport_y).max(0.0);
        let thumb = |track: f32, viewport: f32, max: f32| {
            if max <= viewport {
                1.0
            } else {
                (track * viewport / max).max(20.0).min(track) / track
            }
        };
        let scroll_ratio = |scroll: f32, scrollable: f32| {
            if scrollable <= 0.0 {
                0.0
            } else {
                scroll.clamp(0.0, scrollable) / scrollable
            }
        };

        let playhead_x = self
            .editor
            .get_playback_indicator_screen_x()
            .unwrap_or(view.keyboard_width);

        Some(PianoRollView {
            keyboard_width: view.keyboard_width,
            ruler_height: view.ruler_height,
            key_height: view.zoom_y,
            scroll_y: view.scroll_y,
            visible_keys: view.visible_key_count,
            zoom_x: view.zoom_x,
            scroll_x: view.scroll_x,
            ppq,
            ticks_per_measure,
            playhead_offset: playhead_x - view.keyboard_width,
            scroll_x_ratio: scroll_ratio(view.scroll_x, scrollable_x),
            scroll_y_ratio: scroll_ratio(view.scroll_y, scrollable_y),
            thumb_x_ratio: thumb(track_x, track_x, max_x),
            thumb_y_ratio: thumb(track_y, viewport_y, max_y),
            left_inset,
            // Windows/Linux 的两种标题栏模式在客户端顶部都有 30px 菜单栏行；
            // macOS 原生模式没有（主内容直接从工具栏开始）。
            top_inset: (if cfg!(target_os = "macos") && self.settings.synth.use_native_titlebar {
                0.0
            } else {
                layout::TITLEBAR_H
            }) + self.toolbar.height,
            right_inset,
            bottom_inset: layout::STATUSBAR_H
                + if self.sidebar.automation_panel_visible {
                    self.visual.velocity_panel_height
                } else {
                    0.0
                },
        })
    }

    /// 构造 egui 外壳快照（标题栏 + 工具栏 + 左侧栏 + 状态栏，只读）。
    ///
    /// 测试使用固定视口宽度；真实叠加层走 [`Self::egui_chrome_model_for`]，
    /// 因为工具栏折叠（溢出菜单）依赖实际可用宽度。
    #[cfg(test)]
    pub(crate) fn egui_chrome_model(&self) -> ChromeModel {
        self.egui_chrome_model_for(1280.0)
    }

    /// 原生运行时（`lumino-ui-runtime`）使用的整窗外壳模型。
    ///
    /// 与叠加层共用同一份 `egui_chrome_model_for`；对外公开是为了让
    /// winit + wgpu + egui 直连运行时在不依赖 `Host` 的情况下渲染生产外壳。
    pub fn egui_chrome_model_native(&self, viewport_width: f32) -> ChromeModel {
        self.egui_chrome_model_for(viewport_width)
    }

    /// 原生运行时：应用一帧 egui 动作（菜单 / 工具栏 / 左栏 / 状态栏 / 标题栏）。
    ///
    /// 与 `Host::apply_overlay_actions` 共用同一批 `*_action_message` 映射；
    /// 区别是不经过 iced 窗口上下文，直接 `Root::update`。返回已应用的动作数。
    pub fn apply_egui_chrome_actions(&mut self, ctx: &egui::Context) -> usize {
        use lumino_ui_egui::chrome as ui;

        let mut applied = 0;
        if let Some(index) = ui::menu::take_action(ctx)
            && let Some(message) = self.menu_action_message(index)
        {
            self.update(message);
            applied += 1;
        }
        if let Some(action) = ui::toolbar_popup::take_action(ctx)
            && let Some(message) = self.toolbar_action_message(&action)
        {
            self.update(message);
            applied += 1;
        }
        if let Some(action) = ui::take_panel_action(ctx) {
            let message = self.sidebar_panel_action_message(action);
            self.update(message);
            applied += 1;
        }
        if let Some(action) = ui::take_menu_action(ctx)
            && let Some(message) = self.sidebar_menu_action_message(action)
        {
            self.update(message);
            applied += 1;
        }
        if let Some(action) = ui::statusbar::take_action(ctx) {
            let message = self.statusbar_action_message(action);
            self.update(message);
            applied += 1;
        }
        if let Some(action) = ui::take_titlebar_action(ctx) {
            let message = self.titlebar_action_message(action);
            self.update(message);
            applied += 1;
        }
        if let Some(action) = lumino_ui_egui::piano_roll_context_menu::take_action(ctx) {
            let message = self.piano_roll_context_menu_message(action);
            self.update(message);
            applied += 1;
        }
        if let Some(action) = lumino_ui_egui::editor_action_buttons::take_action(ctx) {
            let message = self.editor_button_message(action);
            self.update(message);
            applied += 1;
        }
        if let Some(bend) = lumino_ui_egui::bend_buttons::take_action(ctx) {
            let message = self.bend_button_message(bend);
            self.update(message);
            applied += 1;
        }
        if let Some(action) = lumino_ui_egui::text_tool::take_action(ctx) {
            let message = self.text_tool_message(action);
            self.update(message);
            applied += 1;
        }
        if let Some(action) = lumino_ui_egui::velocity_panel::take_action(ctx) {
            let message = self.velocity_panel_message(action);
            self.update(message);
            applied += 1;
        }
        applied
    }

    /// 按真实视口宽度构造 egui 外壳快照。
    pub(crate) fn egui_chrome_model_for(&self, viewport_width: f32) -> ChromeModel {
        // 非主窗口：独立对话框 / 进度窗 / 设备警告窗都不画主窗口外壳。
        // 对话框若已支持整窗渲染则返回 `dialog`，否则返回空模型（整块交回 iced），
        // 绝不能把标题栏/工具栏/侧栏 band 画到对话框上。
        if self.state.is_dialog_window {
            let dialog = self.egui_dialog_model();
            let own_titlebar = dialog
                .as_ref()
                .is_some_and(lumino_ui_egui::chrome::DialogModel::draws_own_titlebar);
            let dialog_titlebar = (!self.settings.synth.use_native_titlebar && !own_titlebar)
                .then_some(PopupTitlebarModel {
                    focused: self.window.is_focused,
                });
            return ChromeModel {
                dialog,
                dialog_titlebar,
                ..ChromeModel::default()
            };
        }
        if self.is_progress_window || self.is_device_warning_window {
            let dialog_titlebar =
                (!self.settings.synth.use_native_titlebar).then_some(PopupTitlebarModel {
                    focused: self.window.is_focused,
                });
            return ChromeModel {
                dialog_titlebar,
                ..ChromeModel::default()
            };
        }

        let language = self.settings.display.language;
        let waterfall = self.state.current_mode == crate::titlebar::mode_toggle::AppMode::Waterfall;

        // Windows/Linux 的系统原生标题栏模式在客户端顶部仍保留 30px 菜单栏行
        // （iced `view_native_titlebar`）；叠加层接管后由 egui 画菜单标签与下拉。
        // macOS 原生模式没有这一行（iced 返回空行，主内容不含标题栏）→ 跳过。
        let native = self.settings.synth.use_native_titlebar;
        let egui_menubar = native && !cfg!(target_os = "macos");
        let titlebar_present = !native || egui_menubar;
        let menu_configs = crate::titlebar::menu::menus(language, self.export_material_enabled());
        let menu_bar = menu::menu_bar_model(&menu_configs, language);
        let titlebar = titlebar_present.then(|| TitlebarModel {
            focused: self.window.is_focused,
            menus: menu_bar
                .menus
                .iter()
                .map(|item| item.label.clone())
                .collect(),
            menu: Some(menu_bar),
            menu_only: native,
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
        // 状态栏 GPU 指示器：与 iced `view_status_section` 同源的三开关。
        let gpu_flags = crate::statusbar::GpuStatusFlags {
            enabled: self.settings.synth.gpu_buffer_status_enabled,
            show_governor: self.settings.synth.gpu_governor_metrics_enabled,
            load_mode: self.settings.synth.gpu_load_display,
        };
        let status = self
            .statusbar
            .egui_snapshot(language, note_count, gpu_flags);
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
            // 与生产 `responsive` 同一可用宽度 / 折叠纯函数：溢出菜单内容必须
            // 和工具栏实际折叠结果一致（同宽度必然同结果）。
            let arrangement_mode = self.sidebar.is_arrangement_route();
            let overflow = self.toolbar.compute_overflow(
                toolbar::right_content_width(self, viewport_width),
                arrangement_mode,
            );
            let has_selection = self.editor.selected_notes_count() > 0;
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
                snap_menu: toolbar::snap_menu(&self.toolbar),
                onion_menu: toolbar::onion_menu(&self.toolbar),
                groups: toolbar::groups(&overflow),
                visible_tools: toolbar::visible_tools(&overflow, arrangement_mode),
                collaboration_visible: overflow
                    .visible_groups
                    .contains(&crate::toolbar::overflow::ToolbarGroup::Collaboration),
                more_visible: overflow.has_overflow(),
                overflow_menu: toolbar::overflow_menu(
                    &self.toolbar,
                    &overflow,
                    has_selection,
                    language,
                    arrangement_mode,
                ),
                ppq_editing: self.toolbar.ppq_editing,
                ppq_buffer: self.toolbar.ppq_edit_buffer.clone(),
                ppq_error: self.toolbar.ppq_edit_error.clone(),
                height: toolbar.height,
            })
        } else {
            None
        };

        // 右侧栏模型：图标列 + 已接管的面板内容（I2M / 素材库 / 瀑布流）。
        let right_sidebar = right_sidebar::model(self, titlebar_present, shows_toolbar);

        // 卷帘骨架：默认接管（模型为空时 gfx 继续绘制自己的键盘/标尺）。
        let piano_roll = self.egui_piano_roll_model();

        let tool_panel_top = (if titlebar_present {
            lumino_ui_egui::layout::TITLEBAR_H
        } else {
            0.0
        }) + if shows_toolbar {
            self.toolbar.height
        } else {
            0.0
        };

        ChromeModel {
            dialog: None,
            dialog_titlebar: None,
            titlebar,
            toolbar,
            tool_panel: tool_panel::model(self, sidebar_width, tool_panel_top),
            sidebar,
            right_sidebar,
            piano_roll,
            statusbar: StatusbarModel {
                left: status.left_text,
                left_accent: status.left_accent,
                cpu_percent: metrics_visible.then_some(status.cpu_percent),
                memory_mb: status.memory_mb,
                fps: status.fps,
                right: status.right_text,
                gpu: status
                    .gpu
                    .map(|gpu| lumino_ui_egui::chrome::GpuMetricsModel {
                        queue_ms: gpu.queue_ms,
                        trend: match gpu.trend {
                            crate::statusbar::QueueTrend::Up => {
                                lumino_ui_egui::chrome::GpuQueueTrend::Up
                            }
                            crate::statusbar::QueueTrend::Down => {
                                lumino_ui_egui::chrome::GpuQueueTrend::Down
                            }
                            crate::statusbar::QueueTrend::Flat => {
                                lumino_ui_egui::chrome::GpuQueueTrend::Flat
                            }
                        },
                        load: gpu.load,
                        voice_count: gpu.voice_count,
                        underruns: gpu.underruns,
                        sample_gpu: gpu.sample_gpu,
                        sample_cpu: gpu.sample_cpu,
                        governor_level: gpu.governor_level,
                        dropped: gpu.dropped,
                    }),
            },
        }
    }
}

impl Root {
    /// 状态栏动作 → 生产消息。
    pub(crate) fn statusbar_action_message(
        &self,
        action: lumino_ui_egui::chrome::statusbar::StatusbarAction,
    ) -> crate::Message {
        use lumino_ui_egui::chrome::statusbar::StatusbarAction;
        match action {
            StatusbarAction::OpenMemoryMonitor => {
                crate::toolbar::Event::open_memory_monitor_dialog()
            }
        }
    }
}

#[cfg(test)]
mod audio_export_tests;
#[cfg(test)]
mod batch_edit_tests;
#[cfg(test)]
mod brush_settings_tests;
#[cfg(test)]
mod cloud_browser_tests;
#[cfg(test)]
mod cloud_connect_tests;
#[cfg(test)]
mod collaboration_tests;
#[cfg(test)]
mod custom_precision_tests;
#[cfg(test)]
mod draw_settings_tests;
#[cfg(test)]
mod export_progress_tests;
#[cfg(test)]
mod help_tests;
#[cfg(test)]
mod history_tests;
#[cfg(test)]
mod load_confirm_tests;
#[cfg(test)]
mod memory_monitor_tests;
#[cfg(test)]
mod mixer_tests;
#[cfg(test)]
mod project_settings_tests;
#[cfg(test)]
mod recover_track_tests;
#[cfg(test)]
mod right_sidebar_tests;
#[cfg(test)]
mod save_confirm_tests;
#[cfg(test)]
mod settings_audio_tests;
#[cfg(test)]
mod settings_cloud_tests;
#[cfg(test)]
mod settings_page_tests;
#[cfg(test)]
mod shape_canvas_tests;
#[cfg(test)]
mod shape_drawer_tests;
#[cfg(test)]
mod speed_change_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tool_panel_tests;
#[cfg(test)]
mod video_export_overlay_tests;
#[cfg(test)]
mod video_export_tests;
