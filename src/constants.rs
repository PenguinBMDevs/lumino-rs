//! 应用程序共享常量

/// 文件过滤器常量
pub mod filters {
    /// 音乐文件过滤器 (MID, MIDI, LMPJ)
    pub const MUSIC_FILES: (&str, &[&str]) = ("音乐文件", &["mid", "midi", "lmpj"]);

    /// MIDI 文件过滤器
    pub const MIDI_FILES: (&str, &[&str]) = ("MIDI 文件", &["mid", "midi"]);

    /// Lumino 项目文件过滤器
    pub const LUMINO_PROJECT: (&str, &[&str]) = ("Lumino 项目", &["lmpj"]);

    /// 压缩包文件过滤器
    pub const ARCHIVE_FILES: (&str, &[&str]) = (
        "压缩包文件",
        &[
            "zip", "zipx", "rar", "7z", "tar", "gz", "tgz", "xz", "lzh", "lha", "iso", "zpaq",
        ],
    );

    /// MIDI + 压缩包组合过滤器（方便用户一站式选择）
    pub const MUSIC_AND_ARCHIVE: (&str, &[&str]) = (
        "MIDI 与压缩包文件",
        &[
            "mid", "midi", "lmpj", "zip", "zipx", "rar", "7z", "tar", "gz", "tgz", "xz", "lzh",
            "lha", "iso", "zpaq",
        ],
    );

    /// 所有文件过滤器
    pub const ALL_FILES: (&str, &[&str]) = ("所有文件", &["*"]);
}

#[cfg(test)]
mod tests {
    use super::filters::*;

    #[test]
    fn test_music_filters_contain_midi() {
        assert!(MUSIC_FILES.1.contains(&"mid"));
        assert!(MUSIC_FILES.1.contains(&"midi"));
    }

    #[test]
    fn test_music_filters_contain_lmpj() {
        assert!(MUSIC_FILES.1.contains(&"lmpj"));
    }

    #[test]
    fn test_midi_filter_only_midi() {
        assert!(MIDI_FILES.1.contains(&"mid"));
        assert!(MIDI_FILES.1.contains(&"midi"));
    }

    #[test]
    fn test_archive_filters() {
        assert!(ARCHIVE_FILES.1.contains(&"zip"));
        assert!(ARCHIVE_FILES.1.contains(&"rar"));
        assert!(ARCHIVE_FILES.1.contains(&"7z"));
        assert!(ARCHIVE_FILES.1.contains(&"tar"));
        assert!(ARCHIVE_FILES.1.contains(&"iso"));
        assert!(ARCHIVE_FILES.1.contains(&"zpaq"));
    }

    #[test]
    fn test_music_and_archive_filter() {
        assert!(MUSIC_AND_ARCHIVE.1.contains(&"mid"));
        assert!(MUSIC_AND_ARCHIVE.1.contains(&"zip"));
        assert!(MUSIC_AND_ARCHIVE.1.contains(&"rar"));
    }

    #[test]
    fn test_all_files_wildcard() {
        assert_eq!(ALL_FILES.1, &["*"]);
    }
}
