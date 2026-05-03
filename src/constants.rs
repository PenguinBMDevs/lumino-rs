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
