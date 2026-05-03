//! 应用程序共享常量

/// 文件过滤器常量
pub mod filters {
    /// 音乐文件过滤器 (MID, MIDI, LMPJ, DMS)
    pub const MUSIC_FILES: (&str, &[&str]) = ("音乐文件", &["mid", "midi", "lmpj", "dms"]);

    /// MIDI 文件过滤器
    pub const MIDI_FILES: (&str, &[&str]) = ("MIDI 文件", &["mid", "midi"]);

    /// Lumino 项目文件过滤器
    pub const LUMINO_PROJECT: (&str, &[&str]) = ("Lumino 项目", &["lmpj"]);

    /// Domino 项目文件过滤器
    pub const DOMINO_PROJECT: (&str, &[&str]) = ("Domino 项目", &["dms"]);

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
    fn test_music_filters_contain_dms() {
        assert!(MUSIC_FILES.1.contains(&"dms"));
        assert!(MUSIC_FILES.1.contains(&"lmpj"));
    }

    #[test]
    fn test_midi_filter_only_midi() {
        assert!(MIDI_FILES.1.contains(&"mid"));
        assert!(MIDI_FILES.1.contains(&"midi"));
        assert!(!MIDI_FILES.1.contains(&"dms"));
    }

    #[test]
    fn test_all_files_wildcard() {
        assert_eq!(ALL_FILES.1, &["*"]);
    }
}
