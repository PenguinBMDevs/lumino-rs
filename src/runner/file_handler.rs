/// 文件处理器
pub struct FileHandler {}

impl FileHandler {
    pub fn new() -> Self {
        Self {}
    }

    /// 打开文件对话框并返回选择的路径
    pub fn handle_open_file(&self) -> Option<std::path::PathBuf> {
        rfd::FileDialog::new()
            .add_filter("音乐文件", &["mid", "midi", "lmpj", "dms"])
            .add_filter("MIDI 文件", &["mid", "midi"])
            .add_filter("Lumino 项目", &["lmpj"])
            .add_filter("Domino 项目", &["dms"])
            .add_filter("所有文件", &["*"])
            .pick_file()
    }
}
