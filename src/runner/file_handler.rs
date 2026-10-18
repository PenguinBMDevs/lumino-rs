use crate::constants::filters;

/// 文件处理器
pub struct FileHandler {}

impl FileHandler {
    pub fn new() -> Self {
        Self {}
    }

    /// 打开文件对话框并返回选择的路径
    pub fn handle_open_file(&self) -> Option<std::path::PathBuf> {
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
            .pick_file()
    }
}
