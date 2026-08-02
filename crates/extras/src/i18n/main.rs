//! 主界面翻译（工具栏、标题栏、状态栏、侧边栏、对话框等）
//!
//! 薄入口模块：声明子模块并重导出公共 API。

#[path = "format.rs"]
pub mod format;
#[path = "locale.rs"]
pub mod locale;
#[path = "translations.rs"]
pub mod translations;

pub use format::{
    dot_type_name, eraser_behavior_name, note_precision_name, selection_box_mode_name,
    synth_backend_name, track_add_behavior_name,
};
pub use locale::get;
pub use translations::MainTranslations;

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_core::types::Language;

    #[test]
    fn test_main_translations_zhcn() {
        let t = get(Language::ZhCn);
        assert_eq!(t.play, "播放");
        assert_eq!(t.pause, "暂停");
        assert_eq!(t.menu_file, "文件");
        assert_eq!(t.status_ready, "就绪");
        assert_eq!(t.eb_archive, "存档");
        assert_eq!(t.eb_position, "位置");
        assert_eq!(t.eb_major, "大调");
    }

    #[test]
    fn test_main_translations_enus() {
        let t = get(Language::EnUs);
        assert_eq!(t.play, "Play");
        assert_eq!(t.pause, "Pause");
        assert_eq!(t.menu_file, "File");
        assert_eq!(t.status_ready, "Ready");
        assert_eq!(t.eb_archive, "Archive");
        assert_eq!(t.eb_position, "position");
        assert_eq!(t.eb_major, "Major");
    }

    #[test]
    fn test_main_translations_not_empty() {
        for lang in [Language::ZhCn, Language::EnUs] {
            let t = get(lang);
            assert!(!t.play.is_empty());
            assert!(!t.menu_file.is_empty());
            assert!(!t.undo.is_empty());
            assert!(!t.sidebar_add_track.is_empty());
            assert!(!t.project_title.is_empty());
            assert!(!t.eb_archive.is_empty());
            assert!(!t.eb_position.is_empty());
            assert!(!t.eb_shape.is_empty());
        }
    }
}
