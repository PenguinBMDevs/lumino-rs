//! 生产状态 → egui 外壳视图模型。
//!
//! 只做数据搬运，不含任何渲染逻辑；渲染在 `lumino_ui_egui::chrome::show_bands`。
//! 这样叠加层与离线场景共用同一份绘制代码，也避免 `lumino-ui-egui` 反向依赖 iced。

use lumino_core::storage::config::NoteCountDisplay;
use lumino_core::tr;
use lumino_extras::i18n::main_translations;
use lumino_ui_egui::chrome::piano_roll::PianoRollView;
use lumino_ui_egui::chrome::{
    ChromeModel, DialogModel, RightSidebarModel, RightSidebarPanelKind, RollBar, SaveConfirmModel,
    SettingsSnapshot, SidebarGroup, SidebarModel, SidebarPanel, SidebarTrack, StatusbarModel,
    TitlebarModel, ToolbarModel,
};

use crate::root::Root;
use crate::sidebar::{RollBarButton, Route, Sidebar};
use crate::state::root_state::DialogType;
use crate::toolbar::ToolbarPerfContext;
use lumino_ui_egui::layout;

/// 钢琴卷帘骨架接管开关：默认关闭。
///
/// 生产里卷帘的键盘 / 标尺 / 网格由 `lumino-gfx` 的 wgpu 通道绘制（含主题化
/// 配色），iced canvas 的同名几何被它整块覆盖。egui 骨架在 gfx 退出该区域之前
/// 默认不接管，否则会把用户当前的网格配色盖成 iced 的深色键盘（视觉回归）。
/// 需要实验时置 `LUMINO_EGUI_PIANOROLL=1`。
fn piano_roll_overlay_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED
        .get_or_init(|| std::env::var_os("LUMINO_EGUI_PIANOROLL").is_some_and(|value| value != "0"))
}

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
            top_inset: (if self.settings.synth.use_native_titlebar {
                0.0
            } else {
                layout::TITLEBAR_H
            }) + layout::TOOLBAR_H,
            right_inset,
            bottom_inset: layout::STATUSBAR_H
                + if self.sidebar.automation_panel_visible {
                    self.visual.velocity_panel_height
                } else {
                    0.0
                },
        })
    }

    /// 是否存在由 iced 绘制的弹出层（菜单栏下拉 / 工具栏下拉 / 工具面板）。
    ///
    /// 这些弹层画在 egui 叠加层**下方**，打开时叠加层必须整块让位，否则会被
    /// 不透明 band 盖住——用户能看到菜单项的黑影却点不到。
    fn has_iced_popup(&self) -> bool {
        self.state.is_menu_open
            || self.state.egui_titlebar_latch
            || self.toolbar.snap_picker_open
            || self.toolbar.onion_skin_menu_open
            || self.toolbar.overflow_menu_open
            || self.toolbar.tool_panel_open
            || self.toolbar.brush_dropdown_open
            || self.toolbar.thickness_panel_open
            || self.toolbar.shape_dropdown_open
            || self.toolbar.text_params_open
    }

    /// 已支持整窗接管的对话框（当前仅「未保存更改确认」）。
    ///
    /// 其余对话框（设置 / 导出 / 历史 …）仍由 iced 绘制：整块返回空模型，
    /// 叠加层不画任何东西，避免把主窗口 band 盖到对话框上。
    fn egui_dialog_model(&self) -> Option<DialogModel> {
        // 本层只有深色调色板；浅色主题的对话框仍由 iced 绘制，避免整窗配色错乱。
        if self
            .window
            .theme
            .extended_palette()
            .background
            .weakest
            .color
            .r
            > 0.5
        {
            return None;
        }
        match self.state.dialog_type {
            DialogType::SaveConfirm => Some(DialogModel::SaveConfirm(SaveConfirmModel {
                titlebar: !self.settings.synth.use_native_titlebar,
                message: tr!("是否保留未保存的更改?").to_string(),
                save: tr!("保存").to_string(),
                close: tr!("关闭").to_string(),
                cancel: tr!("取消").to_string(),
            })),
            DialogType::Settings => {
                // 只接管「常规」页且界面语言为中文（其余页面/语言的文案尚未建模）。
                let language = self.settings.display.language;
                if language != lumino_extras::i18n::Language::ZhCn
                    || self.settings.selected_menu_index != 0
                {
                    return None;
                }
                let editing = &self.settings.editing;
                Some(DialogModel::Settings(SettingsSnapshot {
                    selected: self.settings.selected_menu_index,
                    eraser: lumino_extras::i18n::eraser_behavior_name(
                        editing.eraser_behavior,
                        language,
                    )
                    .to_owned(),
                    track_add: lumino_extras::i18n::track_add_behavior_name(
                        editing.track_add_behavior,
                        language,
                    )
                    .to_owned(),
                    log_retention: editing.history_total_limit.to_string(),
                }))
            }
            _ => None,
        }
    }

    /// 构造 egui 外壳快照（标题栏 + 工具栏 + 左侧栏 + 状态栏，只读）。
    pub(crate) fn egui_chrome_model(&self) -> ChromeModel {
        // 非主窗口：独立对话框 / 进度窗 / 设备警告窗都不画主窗口外壳。
        // 对话框若已支持整窗渲染则返回 `dialog`，否则返回空模型（整块交回 iced），
        // 绝不能把标题栏/工具栏/侧栏 band 画到对话框上。
        if self.state.is_dialog_window {
            return ChromeModel {
                dialog: self.egui_dialog_model(),
                ..ChromeModel::default()
            };
        }
        if self.is_progress_window || self.is_device_warning_window {
            return ChromeModel::default();
        }

        // iced 的弹出层（菜单栏下拉、工具栏下拉、工具面板）绘制在叠加层**下方**，
        // 一旦打开必须整块让位，否则会被不透明 band 盖住（菜单看得见点不着）。
        if self.has_iced_popup() {
            return ChromeModel::default();
        }

        let language = self.settings.display.language;
        let translations = main_translations(language);
        let waterfall = self.state.current_mode == crate::titlebar::mode_toggle::AppMode::Waterfall;

        // 系统原生标题栏模式下，顶部 30px 由操作系统绘制，egui 不接管。
        let titlebar_present = !self.settings.synth.use_native_titlebar;
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

        // 右侧栏图标列：仅在实际渲染右侧栏的视图接管（走带 / 瀑布流 / 导出面板
        // / 卷帘关闭时 iced 不渲染右栏）。面板本身保持在 iced（素材扫描、图片
        // 转换等交互复杂），这里只接管 48px 图标列。
        let active_panel = match self.right_sidebar.active_panel {
            crate::right_sidebar::RightSidebarPanel::ImageToMidi => {
                RightSidebarPanelKind::ImageToMidi
            }
            crate::right_sidebar::RightSidebarPanel::Materials => RightSidebarPanelKind::Materials,
            crate::right_sidebar::RightSidebarPanel::PianoWaterfall => {
                RightSidebarPanelKind::PianoWaterfall
            }
        };
        let right_sidebar = if self.right_sidebar_visible() {
            Some(RightSidebarModel {
                panel_visible: self.right_sidebar.panel_visible,
                panel_width: self.right_sidebar.panel_width,
                active: if self.right_sidebar.panel_visible {
                    Some(active_panel)
                } else {
                    None
                },
                top_inset: (if titlebar_present {
                    layout::TITLEBAR_H
                } else {
                    0.0
                }) + if shows_toolbar {
                    layout::TOOLBAR_H
                } else {
                    0.0
                },
            })
        } else {
            None
        };

        let piano_roll = if piano_roll_overlay_enabled() {
            self.egui_piano_roll_model()
        } else {
            None
        };

        ChromeModel {
            dialog: None,
            titlebar,
            toolbar,
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
            },
        }
    }
}

#[cfg(test)]
mod tests;
