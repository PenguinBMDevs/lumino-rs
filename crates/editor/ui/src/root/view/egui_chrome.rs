//! 生产状态 → egui 外壳视图模型。
//!
//! 只做数据搬运，不含任何渲染逻辑；渲染在 `lumino_ui_egui::chrome::show_bands`。
//! 这样叠加层与离线场景共用同一份绘制代码，也避免 `lumino-ui-egui` 反向依赖 上一代。

mod area_palette;
mod arrangement;
mod audio_export_panel;
mod bend_buttons;
mod context_menu;
mod curve_preview;
mod dialogs;
mod editor_buttons;
mod editor_overlay;
mod egg;
mod first_screen;
mod image_to_midi;
mod marquee;
mod mask_panel;
mod material_delete;
mod menu;
mod mixer;
mod palette;
mod remote_cursors;
mod renderer_panel;
mod right_sidebar;
mod settings_audio;
mod settings_pages;
mod settings_ui;
mod shape_canvas;
mod shape_drawer;
mod shape_edit;
mod shape_list;
mod shape_preview;
mod sidebar_panel;
mod tempo_max_bpm;
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
use lumino_core::tr;
use lumino_extras::i18n::main_translations;
use lumino_ui_egui::chrome::piano_roll::PianoRollView;
use lumino_ui_egui::chrome::{
    ChromeModel, DeviceWarningModel, DialogModel, PopupTitlebarModel, ProgressModel, RollBar,
    SidebarGroup, SidebarModel, SidebarPanel, SidebarTrack, StatusbarModel, TitlebarModel,
    ToolbarModel,
};
use lumino_ui_egui::chrome::{SidebarRouteIcon, SidebarRouteItem};
use lumino_ui_egui::widgets::HoverTip;

use crate::root::Root;
use crate::sidebar::{RollBarButton, Route, Sidebar};
use crate::toolbar::ToolbarPerfContext;
use lumino_ui_egui::layout;

/// 上一代 分组 → egui 分组。
fn sidebar_group(group: crate::sidebar::GroupId) -> SidebarGroup {
    match group {
        crate::sidebar::GroupId::PianoRoll => SidebarGroup::PianoRoll,
        crate::sidebar::GroupId::Project => SidebarGroup::Project,
        crate::sidebar::GroupId::Waterfall => SidebarGroup::Waterfall,
        crate::sidebar::GroupId::Renderer => SidebarGroup::Renderer,
    }
}

/// 线性浮点颜色 → egui 颜色（sRGB 直通）。
pub(super) fn color32(color: lumino_ui_core::geom::Color) -> egui::Color32 {
    let to_u8 = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    egui::Color32::from_rgb(to_u8(color.r), to_u8(color.g), to_u8(color.b))
}

impl Sidebar {
    /// 左侧栏本帧是否需要让位给别处绘制（当前恒为 `false`）。
    ///
    /// 历史上这里用于「重命名输入框 / 拖拽插入线 / 颜色·端口·通道选择器 / 宽度拖拽
    /// 仍由上一代 UI 层绘制，叠加层必须让位」的情形；这些交互后来全部迁到 egui
    ///（`chrome/sidebar/menu.rs`、`track_list.rs`），判定位保留，避免将来再引入
    /// 同类状态时把判断散落回各处。
    fn has_transient_ui(&self) -> bool {
        // 已无需要让位的情形：拖拽排序、右键菜单、行内重命名、取色 / 端口 / 通道
        // 选择器、宽度拖拽全部由 egui 绘制并接管。常量 `false` 让上方分支可被优化掉。
        false
    }

    /// 构造左侧栏只读模型；[`Self::has_transient_ui`] 为真时返回 `None`
    ///（叠加层不绘制左栏，避免盖住其他通道的绘制）。
    fn egui_model(
        &self,
        language: lumino_extras::i18n::Language,
        mixer_open: bool,
        waterfall: bool,
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

        // 路由栏条目的可见性 / 点亮规则只有这一处实现，顺序即 `ROUTES` 顺序，
        // 绘制与命中不会再错位。
        let active_group = self.active_group.map(sidebar_group);
        let mut route_items = vec![SidebarRouteItem {
            icon: SidebarRouteIcon::PianoRoll,
            active: self.piano_roll_visible,
            tip: HoverTip::new(tr!("钢琴卷帘"), tr!("打开钢琴卷帘编辑视图")),
        }];
        if active_group == Some(SidebarGroup::PianoRoll) {
            route_items.push(SidebarRouteItem {
                icon: SidebarRouteIcon::File,
                active: file_active,
                tip: HoverTip::new(translations.sidebar_file, tr!("显示工程的全部音轨")),
            });
            route_items.push(SidebarRouteItem {
                icon: SidebarRouteIcon::Automation,
                active: self.automation_panel_visible,
                tip: HoverTip::new(
                    translations.sidebar_automation,
                    tr!("显示当前音轨的自动化曲线"),
                ),
            });
        }
        route_items.push(SidebarRouteItem {
            icon: SidebarRouteIcon::Project,
            active: active_group == Some(SidebarGroup::Project),
            tip: HoverTip::new(tr!("音轨总览"), tr!("切换到工程走带 / 音轨总览视图")),
        });
        route_items.push(SidebarRouteItem {
            icon: SidebarRouteIcon::Waterfall,
            active: waterfall,
            tip: HoverTip::new(tr!("瀑布流播放器"), tr!("切换到瀑布流播放视图")),
        });
        route_items.push(SidebarRouteItem {
            icon: SidebarRouteIcon::Renderer,
            active: active_group == Some(SidebarGroup::Renderer),
            tip: HoverTip::new(tr!("渲染"), tr!("打开渲染入口（视频 / 音频导出）")),
        });
        if active_group == Some(SidebarGroup::Renderer) {
            route_items.push(SidebarRouteItem {
                icon: SidebarRouteIcon::VideoExport,
                active: self.video_export_visible,
                tip: HoverTip::new(tr!("视频导出"), tr!("把工程渲染为视频文件")),
            });
            route_items.push(SidebarRouteItem {
                icon: SidebarRouteIcon::AudioExport,
                active: self.audio_export_visible,
                tip: HoverTip::new(tr!("音频导出"), tr!("把工程渲染为音频文件")),
            });
        }

        Some(SidebarModel {
            panel_visible: self.panel_visible,
            panel_width: self.panel_width,
            piano_roll_visible: self.piano_roll_visible,
            waterfall,
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
            roll_bar_horizontal_tip: HoverTip::new(
                tr!("横向卷帘"),
                tr!("钢琴卷帘横向显示（时间轴水平）"),
            ),
            roll_bar_vertical_tip: HoverTip::new(
                tr!("纵向卷帘"),
                tr!("钢琴卷帘纵向显示（时间轴垂直）"),
            ),
            mixer_tip: HoverTip::new(tr!("混音台"), tr!("打开 / 关闭混音台窗口")),
            solo_tip: HoverTip::new(tr!("独奏"), tr!("只播放此音轨；再次点击取消")),
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
    /// 本帧「无面板」第一屏模型（`None` = 不显示）。
    ///
    /// `project_open` 由宿主传入（`Host::onion_backend_locked()` 背后的
    /// `MidiLoadLock`：真实 MIDI/工程载入为真，启动 / 新建 / 关闭工程为假）：
    /// - 无工程 → 左右分栏，右栏 5 个操作按钮（新建 / 打开文件 / 打开文件夹 /
    ///   设置 / 连接协作房间）；
    /// - 有工程 → 只有品牌块（居中），与旧行为一致。
    pub(crate) fn first_screen_model(
        &self,
        project_open: bool,
    ) -> Option<lumino_ui_egui::chrome::first_screen::FirstScreenModel> {
        use lumino_ui_egui::chrome::first_screen::{
            FirstScreenAction, FirstScreenButton, FirstScreenModel,
        };

        if !self.shows_first_screen() {
            return None;
        }
        let opacity = self.settings.display.first_screen_opacity.clamp(0.0, 1.0);
        if project_open {
            return Some(FirstScreenModel::brand_only(opacity));
        }
        let t = main_translations(self.settings.display.language);
        let button = |action: FirstScreenAction, label: &str| FirstScreenButton {
            action,
            label: label.to_owned(),
        };
        Some(FirstScreenModel {
            opacity,
            actions: vec![
                button(FirstScreenAction::NewProject, t.first_screen_new_project),
                button(FirstScreenAction::OpenProjectFile, t.first_screen_open_file),
                button(
                    FirstScreenAction::OpenProjectFolder,
                    t.first_screen_open_folder,
                ),
                button(FirstScreenAction::OpenSettings, t.first_screen_settings),
                button(
                    FirstScreenAction::OpenCollaboration,
                    t.first_screen_collaboration,
                ),
            ],
        })
    }

    /// 本帧的卷帘骨架模型（生产渲染线程据此决定是否让出标尺）。
    ///
    /// 键盘列与滚动条**只由 egui 绘制**（gfx 网格着色器 discard 键盘列），
    /// 因此只要处于卷帘视图，模型就必须存在；模型为空只允许发生在
    /// 非卷帘视图（走带 / 导出面板 / 卷帘关闭）或画布尚未布局的首帧。
    pub(crate) fn egui_piano_roll_view(&self) -> Option<PianoRollView> {
        self.egui_piano_roll_model()
    }

    /// 卷帘骨架模型。
    ///
    /// 只有「当前不是钢琴卷帘视图」与「画布尚未布局」两种情形返回 `None`。
    /// 洋葱皮 / 协作光标 / 循环区间 / 拍号分段都**不再**让键盘列与滚动条消失：
    /// 它们只影响叠加层；键盘列 / 滚动条 / 标尺全部由 egui 绘制
    /// （gfx 不补键盘，横向标尺也由 egui 按拍号分段画小节号）。
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
        if !self.right_sidebar_visible() {
            return None;
        }
        let view = &self.editor.editor_state.view;
        let canvas = self.editor.editor_state.canvas;
        if canvas.size_x <= 0.0 || canvas.size_y <= 0.0 {
            return None;
        }

        let vertical = self.sidebar.is_vertical_roll();
        // 拍号 → 每小节 tick（默认 4/4）。横向标尺已改为按段计算
        // （`PianoRollView::time_signatures` → `ruler::measure_labels`），
        // 这里只服务纵向卷帘的单段近似；同 tick 重复取最后一条（与 gfx 一致）。
        let (numerator, denominator) = self
            .editor
            .editor_state
            .data
            .time_signatures
            .iter()
            .take_while(|&&(tick, _, _)| tick == 0)
            .last()
            .map(|&(_, numerator, denominator)| (numerator, denominator))
            .unwrap_or((4, 4));
        let ppq = view.ppq;
        let ticks_per_measure =
            f32::from(ppq) * 4.0 * f32::from(numerator) / f32::from(denominator.max(1));

        // 滚动条按 上一代 `ScrollbarWidget::thumb_geometry` 的口径换算成比例：
        // 水平 viewport 缺省为轨道长度；垂直 viewport 为画布高 - 标尺高。
        // 纵向卷帘转置：时间轴视口 = 画布高 - 键盘厚，音高轴视口 = 画布宽。
        let (max_x, max_y) = self.editor.editor_state.max_scroll;
        let grid_h = (canvas.size_y - view.keyboard_width).max(1.0);
        let (track_x, track_y, viewport_x, viewport_y) = if vertical {
            (
                (canvas.size_y + 16.0).max(1.0),
                (canvas.size_x - 4.0).max(1.0),
                grid_h,
                canvas.size_x,
            )
        } else {
            (
                (canvas.size_x + 16.0).max(1.0),
                (canvas.size_y - 4.0).max(1.0),
                (canvas.size_x + 16.0).max(1.0),
                (canvas.size_y - view.ruler_height).max(0.0),
            )
        };
        let scrollable_x = (max_x - viewport_x).max(0.0);
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
            max_scroll_x: max_x,
            max_scroll_y: max_y,
            viewport_y,
            thumb_x_ratio: thumb(track_x, viewport_x, max_x),
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
            vertical,
            time_signatures: self.editor.editor_state.data.time_signatures.clone(),
            // 演奏跟随：键盘列每帧读取播放键色（全零 = 无高亮 / 开关关闭）。
            key_colors: *self.editor.playback_key_colors(),
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
    /// 区别是不经过 上一代 窗口上下文，直接 `Root::update`。返回已应用的动作数。
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
        // 对话框若已支持整窗渲染则返回 `dialog`，否则返回空模型（整块交回 上一代），
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
            // 这两类窗口没有 `DialogType`，内容由 egui 单独绘制（上一代 不再 present）。
            let titlebar = !self.settings.synth.use_native_titlebar;
            let dialog = if self.is_progress_window {
                let (message, progress) = self
                    .progress
                    .as_ref()
                    .map(|(m, p)| (m.clone(), *p))
                    .unwrap_or((tr!("正在初始化...").to_string(), 0.0));
                Some(DialogModel::Progress(ProgressModel {
                    titlebar,
                    title: tr!("处理中...").to_string(),
                    message,
                    progress: progress as f32,
                }))
            } else {
                Some(DialogModel::DeviceWarning(DeviceWarningModel {
                    titlebar,
                    heading: tr!("你的电脑不满足运行 Lumino 的最低要求").to_string(),
                    detail: self.state.device_warning_detail.clone(),
                    suppress: self.state.device_warning_suppress_checked,
                    suppress_label: tr!("不要再提示我").to_string(),
                    quit_label: tr!("确认并关闭").to_string(),
                    continue_label: tr!("放我进去").to_string(),
                }))
            };
            return ChromeModel {
                dialog,
                dialog_titlebar,
                ..ChromeModel::default()
            };
        }

        let language = self.settings.display.language;
        let waterfall = self.state.current_mode == crate::titlebar::mode_toggle::AppMode::Waterfall;

        // Windows/Linux 的系统原生标题栏模式在客户端顶部仍保留 30px 菜单栏行
        // （原 `view_native_titlebar`）；叠加层接管后由 egui 画菜单标签与下拉。
        // macOS 原生模式没有这一行（上一代 返回空行，主内容不含标题栏）→ 跳过。
        let native = self.settings.synth.use_native_titlebar;
        let egui_menubar = native && !cfg!(target_os = "macos");
        let titlebar_present = !native || egui_menubar;
        let menu_configs = crate::titlebar::menu::menus(
            language,
            self.export_material_enabled(),
            self.recent_files(),
        );
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

        // 工具栏：与 上一代 仪表盘共用同一套 tick→BPM/时间换算。
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
        // 状态栏 GPU 指示器：与 上一代 `view_status_section` 同源的三开关。
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
        // 播放器组按钮的点亮态由 `waterfall` 直接传入（与 `current_mode` 同源）。
        let mut sidebar = self
            .sidebar
            .egui_model(language, self.mixer_panel.open, waterfall);
        if let Some(model) = sidebar.as_mut() {
            model.roll_bar_visible = !waterfall && self.sidebar.is_piano_roll_panel();
        }

        // 工具栏模型：`shows_toolbar` 为真时构造（对应需要工具栏的视图）。
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
                precision_label: lumino_extras::i18n::main_translations(language)
                    .precision_label
                    .to_owned(),
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
                tips: toolbar::tips(&self.toolbar, language),
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
                        load: gpu.load,
                        voice_count: gpu.voice_count,
                        underruns: gpu.underruns,
                        sample_gpu: gpu.sample_gpu,
                        sample_cpu: gpu.sample_cpu,
                        governor_level: gpu.governor_level,
                        dropped: gpu.dropped,
                    }),
                memory_tip: HoverTip::new(tr!("内存监控"), tr!("点击查看内存使用明细")),
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
mod about_tests;
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
mod dialog_draw045_tests;
#[cfg(test)]
mod draw_settings_tests;
#[cfg(test)]
mod export_progress_tests;
#[cfg(test)]
mod help_tests;
#[cfg(test)]
mod history_tests;
#[cfg(test)]
mod key_colors_tests;
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
