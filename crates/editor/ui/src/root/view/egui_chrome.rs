//! 生产状态 → egui 外壳视图模型。
//!
//! 只做数据搬运，不含任何渲染逻辑；渲染在 `lumino_ui_egui::chrome::show_bands`。
//! 这样叠加层与离线场景共用同一份绘制代码，也避免 `lumino-ui-egui` 反向依赖 iced。

use lumino_core::storage::config::NoteCountDisplay;
use lumino_extras::i18n::main_translations;
use lumino_ui_egui::chrome::{ChromeModel, StatusbarModel, TitlebarModel};

use crate::root::Root;

impl Root {
    /// 构造 egui 外壳快照（标题栏 + 状态栏，只读）。
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

        let note_count = match self.settings.display.note_count_display {
            NoteCountDisplay::StatusBar => Some(self.editor_note_count()),
            NoteCountDisplay::ProjectSettings => None,
        };
        let status = self.statusbar.egui_snapshot(language, note_count);
        // 生产 `perf_section` 仅在 `fps` 已采样时渲染；同组隐藏 CPU 数值。
        let metrics_visible = status.fps.is_some();

        ChromeModel {
            titlebar,
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
