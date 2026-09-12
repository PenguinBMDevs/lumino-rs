//! 视频导出渲染模式 —— `RenderMode` 枚举与各 impl。
//!
//! 从 `types.rs` 拆分而来（文件行数红线），由 `types.rs` 通过 `pub use` 对外统一暴露。

use super::impl_unit_enum_from_str;

/// 视频导出渲染模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderMode {
    /// Lumino瀑布流渲染（默认模式，音符随时间向下流动）
    #[default]
    Waterfall,
    /// Lumino卷帘渲染（传统钢琴卷帘样式）
    NoteRectangle,
    /// MIDITrail 风格（3D MIDI 轨迹可视化）
    MIDITrail,
    /// 计数器渲染（不绘制卷帘，仅在画面上显示变化的统计数据文本）
    NoteCounter,
    /// 数据曲线渲染（绘制统计数据随时间的折线图，参考 MIDIGraphRenderer 移植）
    DataCurve,
    /// MidiConsole 风格（复刻 MidiConsole 的终端像素网格：逐通道半块键盘条 + 控制面板，参考 MidiConsole by Zacksony）
    MidiConsole,
}

impl RenderMode {
    /// 全部渲染模式，顺序即导出面板下拉列表的展示顺序。
    ///
    /// UI 列表统一由此生成（经 `Display` 转文本），收敛显示名的单一权威来源，
    /// 避免改名时只改一处导致界面选项与解析表脱节。
    pub const ALL: [RenderMode; 6] = [
        RenderMode::Waterfall,
        RenderMode::NoteRectangle,
        RenderMode::MIDITrail,
        RenderMode::NoteCounter,
        RenderMode::DataCurve,
        RenderMode::MidiConsole,
    ];

    /// 导出到渲染线程用的规范字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            RenderMode::Waterfall => "waterfall",
            RenderMode::NoteRectangle => "note_rectangle",
            RenderMode::MIDITrail => "miditrail",
            RenderMode::NoteCounter => "note_counter",
            RenderMode::DataCurve => "data_curve",
            RenderMode::MidiConsole => "midi_console",
        }
    }
}

impl std::fmt::Display for RenderMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderMode::Waterfall => f.write_str("Lumino瀑布流"),
            RenderMode::NoteRectangle => f.write_str("Lumino卷帘"),
            RenderMode::MIDITrail => f.write_str("MIDITrail"),
            RenderMode::NoteCounter => f.write_str("计数器"),
            RenderMode::DataCurve => f.write_str("数据曲线"),
            RenderMode::MidiConsole => f.write_str("MidiConsole"),
        }
    }
}

impl_unit_enum_from_str!(RenderMode, "未知渲染模式: {input}", {
    Waterfall => ["waterfall", "瀑布流", "Lumino瀑布流"],
    // "音符矩形" 为旧显示名，保留解析兼容（旧配置/脚本仍可读）。
    NoteRectangle => ["note_rectangle", "音符矩形", "Lumino卷帘"],
    MIDITrail => ["miditrail", "MIDITrail"],
    NoteCounter => ["note_counter", "计数器", "NoteCounter"],
    DataCurve => ["data_curve", "数据曲线", "DataCurve"],
    MidiConsole => ["midi_console", "MidiConsole"],
});

#[cfg(test)]
mod tests {
    use super::*;

    /// 显示名：NoteRectangle 的对外名称应为「Lumino卷帘」。
    #[test]
    fn test_note_rectangle_display_is_lumino_roll() {
        assert_eq!(RenderMode::NoteRectangle.to_string(), "Lumino卷帘");
    }

    /// 解析兼容：新显示名 / 旧显示名 / 规范字符串三种写法均可解析。
    #[test]
    fn test_note_rectangle_accepts_new_legacy_and_canonical_names() {
        for raw in ["Lumino卷帘", "音符矩形", "note_rectangle"] {
            assert_eq!(
                raw.parse::<RenderMode>().expect("应可解析为 NoteRectangle"),
                RenderMode::NoteRectangle,
                "解析失败: {raw}"
            );
        }
    }

    /// `ALL` 与 `Display`/`FromStr` 闭环：UI 列表来源与解析表不得脱节。
    #[test]
    fn test_all_variants_roundtrip_display_parse() {
        for mode in RenderMode::ALL {
            let name = mode.to_string();
            assert_eq!(
                name.parse::<RenderMode>().expect("Display 输出应可解析"),
                mode,
                "往返失败: {name}"
            );
        }
    }
}
