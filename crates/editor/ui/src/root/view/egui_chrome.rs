//! 生产状态 → egui 外壳视图模型。
//!
//! 只做数据搬运，不含任何渲染逻辑；渲染在 `lumino_ui_egui::chrome::show_bands`。
//! 这样叠加层与离线场景共用同一份绘制代码，也避免 `lumino-ui-egui` 反向依赖 iced。

use lumino_core::storage::config::NoteCountDisplay;
use lumino_extras::i18n::main_translations;
use lumino_ui_egui::chrome::{ChromeModel, StatusbarModel, TitlebarModel, ToolbarModel};

use crate::root::Root;
use crate::toolbar::ToolbarPerfContext;

impl Root {
    /// 构造 egui 外壳快照（标题栏 + 工具栏 + 状态栏，只读）。
    pub(crate) fn egui_chrome_model(&self) -> ChromeModel {
        let language = self.settings.display.language;
        let translations = main_translations(language);

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
        let shows_toolbar = self.state.current_mode
            != crate::titlebar::mode_toggle::AppMode::Waterfall
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

        ChromeModel {
            titlebar,
            toolbar: shows_toolbar.then(|| ToolbarModel {
                left_inset: self.sidebar.width() as f32,
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
            }),
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
