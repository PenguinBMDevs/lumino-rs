use crate::constants::filters;

/// 文件处理器
pub struct FileHandler {}

impl FileHandler {
    pub fn new() -> Self {
        Self {}
    }

    /// 打开文件对话框并返回选择的路径
    pub fn handle_open_file(&self) -> Option<std::path::PathBuf> {
        rfd::FileDialog::new()
            .add_filter(filters::MUSIC_AND_ARCHIVE.0, filters::MUSIC_AND_ARCHIVE.1)
            .add_filter(filters::MUSIC_FILES.0, filters::MUSIC_FILES.1)
            .add_filter(filters::MIDI_FILES.0, filters::MIDI_FILES.1)
            .add_filter(filters::LUMINO_PROJECT.0, filters::LUMINO_PROJECT.1)
            .add_filter(filters::ARCHIVE_FILES.0, filters::ARCHIVE_FILES.1)
            .add_filter(filters::ALL_FILES.0, filters::ALL_FILES.1)
            .pick_file()
    }
}
