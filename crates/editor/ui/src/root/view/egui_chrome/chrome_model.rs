//! 主窗口 / 对话框窗口的 egui 外壳模型。
//!
//! `egui_chrome_model_for` 只做窗口类型分流；主窗口按「标题栏 → 工具栏 → 侧栏 →
//! 状态栏」分段构造，避免一个函数同时处理三种窗口。

use lumino_core::i18n::{Language, main_translations};
use lumino_core::storage::config::NoteCountDisplay;
use lumino_core::tr;
use lumino_ui_egui::chrome::{
    ChromeModel, DeviceWarningModel, DialogModel, PopupTitlebarModel, ProgressModel,
    StatusbarModel, TitlebarModel, ToolbarModel,
};
use lumino_ui_egui::layout;
use lumino_ui_egui::widgets::HoverTip;

use crate::root::Root;
use crate::toolbar::ToolbarPerfContext;

use super::{menu, right_sidebar, tool_panel, toolbar};

/// 主窗口外壳的共享判定，避免在多个构造函数里重复推导。
struct WindowChrome {
    titlebar: Option<TitlebarModel>,
    titlebar_present: bool,
    shows_toolbar: bool,
    sidebar_width: f32,
    waterfall: bool,
}

impl Root {
    /// 按真实视口宽度构造 egui 外壳快照。
    pub(crate) fn egui_chrome_model_for(&self, viewport_width: f32) -> ChromeModel {
        // 非主窗口：独立对话框 / 进度窗 / 设备警告窗都不画主窗口外壳。
        // 对话框若已支持整窗渲染则返回 `dialog`，否则返回空模型（整块交回 上一代），
        // 绝不能把标题栏/工具栏/侧栏 band 画到对话框上。
        if self.state.is_dialog_window {
            return self.dialog_window_model();
        }
        if self.is_progress_window || self.is_device_warning_window {
            return self.progress_or_warning_model();
        }
        self.main_window_model(viewport_width)
    }

    /// 独立对话框窗口：只给对话框自己的模型与弹层标题栏。
    fn dialog_window_model(&self) -> ChromeModel {
        let dialog = self.egui_dialog_model();
        let own_titlebar = dialog.as_ref().is_some_and(DialogModel::draws_own_titlebar);
        let dialog_titlebar = (!self.settings.synth.use_native_titlebar && !own_titlebar)
            .then_some(PopupTitlebarModel {
                focused: self.window.is_focused,
            });
        ChromeModel {
            dialog,
            dialog_titlebar,
            ..ChromeModel::default()
        }
    }

    /// 进度窗 / 设备警告窗：没有 `DialogType`，内容由 egui 单独绘制。
    fn progress_or_warning_model(&self) -> ChromeModel {
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
        ChromeModel {
            dialog,
            dialog_titlebar,
            ..ChromeModel::default()
        }
    }

    /// 主窗口：标题栏 / 工具栏 / 侧栏 / 状态栏 / 卷帘骨架。
    fn main_window_model(&self, viewport_width: f32) -> ChromeModel {
        let language = self.settings.display.language;
        let chrome = self.window_chrome(language);

        let note_count = match self.settings.display.note_count_display {
            NoteCountDisplay::StatusBar => Some(self.editor_note_count()),
            NoteCountDisplay::ProjectSettings => None,
        };
        let statusbar = self.egui_statusbar_model(language, note_count);

        // 左侧栏：瀑布流模式同样存在（`view_waterfall_fullscreen` 也渲染左栏）。
        // 播放器组按钮的点亮态由 `waterfall` 直接传入（与 `current_mode` 同源）。
        let mut sidebar =
            self.sidebar
                .egui_model(language, self.mixer_panel.open, chrome.waterfall);
        if let Some(model) = sidebar.as_mut() {
            model.roll_bar_visible = !chrome.waterfall && self.sidebar.is_piano_roll_panel();
        }

        let toolbar_model = self.egui_toolbar_model(
            viewport_width,
            chrome.sidebar_width,
            chrome.shows_toolbar,
            language,
        );

        // 右侧栏模型：图标列 + 已接管的面板内容（I2M / 素材库 / 瀑布流）。
        let right_sidebar =
            right_sidebar::model(self, chrome.titlebar_present, chrome.shows_toolbar);

        // 卷帘骨架：默认接管（模型为空时 gfx 继续绘制自己的键盘/标尺）。
        let piano_roll = self.egui_piano_roll_model();

        let tool_panel_top = (if chrome.titlebar_present {
            layout::TITLEBAR_H
        } else {
            0.0
        }) + if chrome.shows_toolbar {
            self.toolbar.height
        } else {
            0.0
        };

        ChromeModel {
            dialog: None,
            dialog_titlebar: None,
            titlebar: chrome.titlebar,
            toolbar: toolbar_model,
            tool_panel: tool_panel::model(self, chrome.sidebar_width, tool_panel_top),
            sidebar,
            right_sidebar,
            piano_roll,
            statusbar,
        }
    }

    /// 标题栏 / 工具栏可见性与侧栏宽度：主窗口内多处共用。
    fn window_chrome(&self, language: Language) -> WindowChrome {
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

        WindowChrome {
            titlebar,
            titlebar_present,
            shows_toolbar,
            sidebar_width,
            waterfall,
        }
    }

    /// 工具栏模型：`shows_toolbar` 为真时构造（对应需要工具栏的视图）。
    fn egui_toolbar_model(
        &self,
        viewport_width: f32,
        sidebar_width: f32,
        shows_toolbar: bool,
        language: Language,
    ) -> Option<ToolbarModel> {
        if !shows_toolbar {
            return None;
        }
        // 工具栏：与 上一代 仪表盘共用同一套 tick→BPM/时间换算。
        let perf = ToolbarPerfContext {
            playback_tick: self.editor.playback_position,
            ppq: self.editor.editor_state.view.ppq,
            tempo_points: &self.editor.editor_state.data.tempo_points,
        };
        let snapshot = self.toolbar.egui_snapshot(&perf);
        // 与生产 `responsive` 同一可用宽度 / 折叠纯函数：溢出菜单内容必须
        // 和工具栏实际折叠结果一致（同宽度必然同结果）。
        let arrangement_mode = self.sidebar.is_arrangement_route();
        // 精度选择器宽度随吸附文本变化：折叠预算与绘制共用同一函数，
        // 否则按钮变宽后总宽不跟着变，会压住右侧分组。
        let overflow = self.toolbar.compute_overflow_localized(
            toolbar::right_content_width(self, viewport_width),
            arrangement_mode,
            language,
        );
        let has_selection = self.editor.selected_notes_count() > 0;
        Some(ToolbarModel {
            left_inset: sidebar_width,
            bpm: snapshot.bpm,
            ppq: snapshot.ppq,
            position: snapshot.position,
            time: snapshot.time,
            snap: snapshot.snap,
            precision_label: main_translations(language).precision_label.to_owned(),
            tool: snapshot.tool,
            draw_panel_open: snapshot.draw_panel_open,
            playing: snapshot.playing,
            looping: snapshot.looping,
            recording: snapshot.recording,
            onion: snapshot.onion,
            auto_scroll: snapshot.auto_scroll,
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
            bpm_editing: self.toolbar.bpm_editing,
            bpm_buffer: self.toolbar.bpm_edit_buffer.clone(),
            bpm_error: self.toolbar.bpm_edit_error.clone(),
            height: snapshot.height,
            tips: toolbar::tips(&self.toolbar, language),
        })
    }

    /// 状态栏模型：生产 `perf_section` 仅在 `fps` 已采样时渲染；同组隐藏 CPU 数值。
    fn egui_statusbar_model(
        &self,
        language: Language,
        note_count: Option<usize>,
    ) -> StatusbarModel {
        // 状态栏 GPU 指示器：与 上一代 `view_status_section` 同源的三开关。
        let gpu_flags = crate::statusbar::GpuStatusFlags {
            enabled: self.settings.synth.gpu_buffer_status_enabled,
            show_governor: self.settings.synth.gpu_governor_metrics_enabled,
            load_mode: self.settings.synth.gpu_load_display,
        };
        let status = self
            .statusbar
            .egui_snapshot(language, note_count, gpu_flags);
        let metrics_visible = status.fps.is_some();
        StatusbarModel {
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
        }
    }
}
