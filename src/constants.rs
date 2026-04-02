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

/// MIDI/音频相关常量
pub mod midi {
    /// 默认 PPQ (Pulses Per Quarter note)
    pub const DEFAULT_PPQ: u16 = 480;

    /// 标准 MIDI 文件默认 PPQ
    pub const STANDARD_PPQ: u16 = 960;
}

/// UI 相关常量
pub mod ui {
    /// 默认窗口宽度
    pub const DEFAULT_WINDOW_WIDTH: u32 = 1280;

    /// 默认窗口高度
    pub const DEFAULT_WINDOW_HEIGHT: u32 = 720;
}
