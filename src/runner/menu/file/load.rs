//! Runner 文件菜单：加载与导入

use std::path::PathBuf;

use crate::runner::{RunnerInner, async_helper::run_async_task};

impl RunnerInner {
    /// 打开文件
    pub(super) fn handle_open_file(&mut self) {
        // 使用 FileHandler 打开文件对话框
        let Some(path) = self.file_state.file_handler.handle_open_file() else {
            return;
        };

        // 解锁调色板，新 MIDI 加载后 MidiParsed 会重新锁定
        lumino_core::palette::unlock_palette();

        tracing::info!("开始加载 MIDI 文件：{:?}", path);
        self.load_midi_file(path);
    }

    /// 加载 MIDI 文件
    pub(crate) fn load_midi_file(&self, path: PathBuf) {
        tracing::info!("开始后台加载 MIDI 文件：{:?}", path);
        let progress_cb = self.window_state.progress_cb.clone();
        tokio::spawn(async move {
            run_async_task(
                lumino_midi_loader::loader::load_parsed_midi(path, Some(&progress_cb)),
                |parsed| {
                    lumino_ui::event::Event::menu_file(
                        lumino_ui::event::menu::file::Event::midi_parsed(std::sync::Arc::new(
                            parsed,
                        )),
                    )
                },
                |e| {
                    lumino_ui::event::Event::menu_file(
                        lumino_ui::event::menu::file::Event::midi_parse_error(e),
                    )
                },
            )
            .await;
        });
    }

    /// 导入文件
    pub(super) fn handle_import_files(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(
                crate::constants::filters::MUSIC_FILES.0,
                crate::constants::filters::MUSIC_FILES.1,
            )
            .add_filter(
                crate::constants::filters::MIDI_FILES.0,
                crate::constants::filters::MIDI_FILES.1,
            )
            .add_filter(
                crate::constants::filters::LUMINO_PROJECT.0,
                crate::constants::filters::LUMINO_PROJECT.1,
            )
            .add_filter(
                crate::constants::filters::ALL_FILES.0,
                crate::constants::filters::ALL_FILES.1,
            )
            .pick_file()
        else {
            return;
        };

        // 解锁调色板，新 MIDI 加载后 MidiParsed 会重新锁定
        lumino_core::palette::unlock_palette();

        tracing::info!("开始导入 MIDI 文件：{:?}", path);
        // 复用 load_midi_file 的后台加载逻辑，避免与 handle_open_file 重复
        self.load_midi_file(path);
    }

    /// 将 MIDI 数据导入到编辑器
    pub(super) fn import_midi_to_editor(&mut self, parsed: &lumino_midi_loader::ParsedMidi) {
        {
            let ui = self.window_state.window.ui_mut();
            self.midi_state
                .midi_handler
                .import_midi_to_editor(ui, parsed);
        }

        // MIDI 导入后，为播放管理器绑定一个独立的 MIDI 输出连接
        if let Some(output) = self.midi_state.midi.create_additional_output() {
            self.window_state
                .window
                .ui_mut()
                .set_playback_midi_output(output);
            tracing::info!("Playback MIDI output connected");
        } else {
            tracing::warn!("Failed to create playback MIDI output connection");
        }
    }
}
