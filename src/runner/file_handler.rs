use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use lumino_core::trf;

use crate::constants::filters;

/// 文件处理器
pub struct FileHandler {
    dialog_open: Arc<AtomicBool>,
}

pub(crate) struct FileDialogGuard {
    dialog_open: Arc<AtomicBool>,
}

impl Drop for FileDialogGuard {
    fn drop(&mut self) {
        self.dialog_open.store(false, Ordering::Release);
    }
}

impl FileHandler {
    pub fn new() -> Self {
        Self {
            dialog_open: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) fn try_acquire_dialog(&self) -> Option<FileDialogGuard> {
        self.dialog_open
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| FileDialogGuard {
                dialog_open: Arc::clone(&self.dialog_open),
            })
    }

    fn dialog(&self) -> lumino_platform::dialog::FileDialog {
        lumino_platform::dialog::FileDialog::new()
            .add_filter(
                filters::music_and_archive_filter(),
                filters::MUSIC_AND_ARCHIVE_EXTS,
            )
            .add_filter(filters::music_files_filter(), filters::MUSIC_FILES_EXTS)
            .add_filter(filters::midi_files_filter(), filters::MIDI_FILES_EXTS)
            .add_filter(
                filters::lumino_project_filter(),
                filters::LUMINO_PROJECT_EXTS,
            )
            .add_filter(filters::archive_files_filter(), filters::ARCHIVE_FILES_EXTS)
            .add_filter(filters::all_files_filter(), filters::ALL_FILES_EXTS)
    }

    /// 在后台线程打开文件对话框，选择结果通过 UI 事件队列返回。
    pub fn pick_file_async(
        &self,
        selected_event: fn(PathBuf) -> lumino_ui::event::menu::file::Event,
    ) {
        let Some(guard) = self.try_acquire_dialog() else {
            return;
        };
        let dialog = self.dialog();
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || dialog.pick_file()).await;
            match result {
                Ok(Some(path)) => {
                    lumino_ui::event::emit(lumino_ui::event::Event::menu_file(selected_event(path)))
                }
                Ok(None) => {}
                Err(e) => lumino_ui::event::emit(lumino_ui::event::Event::menu_file(
                    lumino_ui::event::menu::file::Event::MidiParseError(trf!(
                        "文件对话框任务失败: {e}",
                        e = e
                    )),
                )),
            }
            drop(guard);
        });
    }

    /// 在后台线程打开**文件夹**对话框，选择结果通过 UI 事件队列返回。
    ///
    /// 与 [`Self::pick_file_async`] 共用同一个「同时只允许一个对话框」的闸门：
    /// 文件 / 文件夹两个入口不能各自弹一个系统对话框。
    pub fn pick_folder_async(
        &self,
        selected_event: fn(PathBuf) -> lumino_ui::event::menu::file::Event,
    ) {
        let Some(guard) = self.try_acquire_dialog() else {
            return;
        };
        let dialog = self.dialog();
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || dialog.pick_folder()).await;
            match result {
                Ok(Some(path)) => {
                    lumino_ui::event::emit(lumino_ui::event::Event::menu_file(selected_event(path)))
                }
                Ok(None) => {}
                Err(e) => lumino_ui::event::emit(lumino_ui::event::Event::menu_file(
                    lumino_ui::event::menu::file::Event::MidiParseError(trf!(
                        "文件夹对话框任务失败: {e}",
                        e = e
                    )),
                )),
            }
            drop(guard);
        });
    }
}

impl Default for FileHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_dialog_allows_only_one_in_flight() {
        let handler = FileHandler::new();
        let guard = handler
            .try_acquire_dialog()
            .expect("首个文件对话框应可打开");
        assert!(handler.try_acquire_dialog().is_none());

        drop(guard);

        assert!(handler.try_acquire_dialog().is_some());
    }
}
