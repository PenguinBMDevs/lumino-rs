//! Runner 文件菜单：导出 MIDI 文件

use crate::runner::RunnerInner;

use super::editor_midi;
use super::helpers;

impl RunnerInner {
    /// 导出当前编辑器内容为 MIDI 文件（.mid）
    ///
    /// 与"保存"的区别：只生成独立的 MIDI 文件，不修改 `current_midi_source`，
    /// 因此不会影响后续的 Ctrl+S 覆盖保存路径。
    pub(super) fn handle_export_midi(&mut self) {
        // 自动提交当前编辑（ghost 方案：松手即提交），
        // 保证导出的数据包含用户正在编辑（拖动/绘制/调整大小）的音符。
        let committed = self
            .window_state
            .window
            .ui_mut()
            .root_mut()
            .editor
            .commit_current_edit();
        if committed {
            tracing::debug!("导出 MIDI 前自动提交编辑");
        }

        // 从编辑器内容构建导出数据（无音符/文档时返回 None）
        let Some(export_data) = editor_midi::build_midi_export_data_from_editor(self, true) else {
            let language = self.window_state.window.ui().settings().display.language;
            let msg = lumino_extras::i18n::main_translations(language)
                .status_no_midi_content
                .to_string();
            self.window_state
                .window
                .ui_mut()
                .set_status_message(Some(msg));
            tracing::info!("导出 MIDI：没有可导出的内容");
            return;
        };

        let file_stem = self
            .midi_state
            .current_midi_source
            .as_ref()
            .map(|p| helpers::get_file_stem(std::path::Path::new(p)))
            .unwrap_or_else(|| "untitled".to_string());

        let Some(save_path) = rfd::FileDialog::new()
            .add_filter(
                crate::constants::filters::MIDI_FILES.0,
                crate::constants::filters::MIDI_FILES.1,
            )
            .set_file_name(format!("{file_stem}.mid"))
            .save_file()
        else {
            return;
        };

        match lumino_export::export_midi(&save_path, &export_data) {
            Ok(()) => {
                tracing::info!("MIDI 已导出: {:?}", save_path);
                let language = self.window_state.window.ui().settings().display.language;
                let msg = lumino_extras::i18n::main_translations(language)
                    .status_midi_exported
                    .to_string();
                self.window_state
                    .window
                    .ui_mut()
                    .set_status_message(Some(msg));
            }
            Err(e) => {
                tracing::error!("导出 MIDI 失败: {}", e);
                let language = self.window_state.window.ui().settings().display.language;
                let msg = format!(
                    "{}：{e}",
                    lumino_extras::i18n::main_translations(language).status_save_failed
                );
                self.window_state
                    .window
                    .ui_mut()
                    .set_status_message(Some(msg));
            }
        }
    }
}
