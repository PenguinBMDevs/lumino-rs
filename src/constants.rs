//! 应用程序共享常量

/// 文件过滤器常量
///
/// 过滤器**标签**会显示在系统文件对话框里，必须随界面语言变化，因此改为
/// 函数式取词（`tr!` 运行时查表）；扩展名列表与语言无关，仍保留为
/// `*_EXTS` 常量。调用点请写 `add_filter(xxx_filter(), XXX_EXTS)`。
pub mod filters {
    use lumino_core::tr;

    /// 音乐文件过滤器标签 (MID, MIDI, LMPJ)
    pub fn music_files_filter() -> &'static str {
        tr!("音乐文件")
    }

    /// 音乐文件过滤器扩展名 (MID, MIDI, LMPJ)
    pub const MUSIC_FILES_EXTS: &[&str] = &["mid", "midi", "lmpj"];

    /// MIDI 文件过滤器标签
    pub fn midi_files_filter() -> &'static str {
        tr!("MIDI 文件")
    }

    /// MIDI 文件过滤器扩展名
    pub const MIDI_FILES_EXTS: &[&str] = &["mid", "midi"];

    /// Lumino 项目文件过滤器标签
    pub fn lumino_project_filter() -> &'static str {
        tr!("Lumino 项目")
    }

    /// Lumino 项目文件过滤器扩展名
    pub const LUMINO_PROJECT_EXTS: &[&str] = &["lmpj"];

    /// Lumino 素材文件过滤器标签 (.lmmaterial)
    pub fn lumino_material_filter() -> &'static str {
        tr!("Lumino 素材")
    }

    /// Lumino 素材文件过滤器扩展名
    pub const LUMINO_MATERIAL_EXTS: &[&str] = &["lmmaterial"];

    /// 压缩包文件过滤器标签
    pub fn archive_files_filter() -> &'static str {
        tr!("压缩包文件")
    }

    /// 压缩包文件过滤器扩展名
    pub const ARCHIVE_FILES_EXTS: &[&str] = &[
        "zip", "zipx", "rar", "7z", "tar", "gz", "tgz", "xz", "lzh", "lha", "iso",
    ];

    /// MIDI + 压缩包组合过滤器标签（方便用户一站式选择）
    pub fn music_and_archive_filter() -> &'static str {
        tr!("MIDI 与压缩包文件")
    }

    /// MIDI + 压缩包组合过滤器扩展名
    pub const MUSIC_AND_ARCHIVE_EXTS: &[&str] = &[
        "mid", "midi", "lmpj", "zip", "zipx", "rar", "7z", "tar", "gz", "tgz", "xz", "lzh", "lha",
        "iso",
    ];

    /// 所有文件过滤器标签
    pub fn all_files_filter() -> &'static str {
        tr!("所有文件")
    }

    /// 所有文件过滤器扩展名
    pub const ALL_FILES_EXTS: &[&str] = &["*"];
}

#[cfg(test)]
mod tests {
    use super::filters::*;

    #[test]
    fn test_music_filters_contain_midi() {
        assert!(MUSIC_FILES_EXTS.contains(&"mid"));
        assert!(MUSIC_FILES_EXTS.contains(&"midi"));
    }

    #[test]
    fn test_music_filters_contain_lmpj() {
        assert!(MUSIC_FILES_EXTS.contains(&"lmpj"));
    }

    #[test]
    fn test_midi_filter_only_midi() {
        assert!(MIDI_FILES_EXTS.contains(&"mid"));
        assert!(MIDI_FILES_EXTS.contains(&"midi"));
    }

    #[test]
    fn test_archive_filters() {
        assert!(ARCHIVE_FILES_EXTS.contains(&"zip"));
        assert!(ARCHIVE_FILES_EXTS.contains(&"rar"));
        assert!(ARCHIVE_FILES_EXTS.contains(&"7z"));
        assert!(ARCHIVE_FILES_EXTS.contains(&"tar"));
        assert!(ARCHIVE_FILES_EXTS.contains(&"iso"));
    }

    #[test]
    fn test_music_and_archive_filter() {
        assert!(MUSIC_AND_ARCHIVE_EXTS.contains(&"mid"));
        assert!(MUSIC_AND_ARCHIVE_EXTS.contains(&"zip"));
        assert!(MUSIC_AND_ARCHIVE_EXTS.contains(&"rar"));
    }

    #[test]
    fn test_all_files_wildcard() {
        assert_eq!(ALL_FILES_EXTS, &["*"]);
    }
}
