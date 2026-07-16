//! 工具栏按钮描述配置（外置）
//!
//! 底部状态栏在鼠标悬停工具栏按钮时，于左侧描述区显示
//! `按钮名 - {解释说明}` 格式的文本。
//!
//! - `按钮名` 取自项目既有 i18n 体系（`MainTranslations`），随语言切换。
//! - `{解释说明}` 由本文件的 `DESC_ZH` / `DESC_EN` 表提供，目前以 `{...}`
//!   占位，**留待人工填写**（中英文均需填写）。
//!
//! 新增工具栏按钮时，只需在此处补充 `ButtonId` 变体并填写对应描述，
//! 编译期即可保证不漏配。

use lumino_core::i18n::{Language, MainTranslations, main_translations};

/// 工具栏按钮的稳定角色标识
///
/// 与具体按钮实例一一对应。动态文本按钮（如循环开/关、移调 ±1/±12）
/// 使用中性角色名（见 `button_name`），以保证 i18n 下名称稳定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ButtonId {
    SkipBackward,
    Play,
    Pause,
    SkipForward,
    Record,
    RecordStop,
    Pointer,
    Pencil,
    Eraser,
    Curve,
    Quantize,
    Speed,
    FlipVertical,
    FlipHorizontal,
    Split,
    Glue,
    Tie,
    TransposeDown,
    TransposeUp,
    Undo,
    Redo,
    Loop,
    AutoScroll,
    Collaboration,
    More,
}

/// 按钮名（实际显示文本），随语言切换
fn button_name(id: ButtonId, t: &MainTranslations) -> &'static str {
    match id {
        ButtonId::SkipBackward => t.skip_backward,
        ButtonId::Play => t.play,
        ButtonId::Pause => t.pause,
        ButtonId::SkipForward => t.skip_forward,
        ButtonId::Record => t.record_start,
        ButtonId::RecordStop => t.record_stop,
        ButtonId::Pointer => t.tool_pointer,
        ButtonId::Pencil => t.tool_pencil,
        ButtonId::Eraser => t.tool_eraser,
        ButtonId::Curve => t.tool_curve,
        ButtonId::Quantize => t.tool_quantize,
        ButtonId::Speed => t.tool_speed,
        ButtonId::FlipVertical => t.tool_flip_vertical,
        ButtonId::FlipHorizontal => t.tool_flip_horizontal,
        ButtonId::Split => t.tool_split,
        ButtonId::Glue => t.tool_glue,
        ButtonId::Tie => t.tool_tie,
        ButtonId::TransposeDown => t.tool_transpose_down,
        ButtonId::TransposeUp => t.tool_transpose_up,
        ButtonId::Undo => t.undo,
        ButtonId::Redo => t.redo,
        // 动态按钮使用中性角色名，保证 i18n 下稳定
        ButtonId::Loop => t.toggle_loop_tooltip,
        ButtonId::AutoScroll => t.auto_scroll_tooltip,
        ButtonId::Collaboration => t.collaboration_tooltip,
        ButtonId::More => t.toolbar_more,
    }
}

/// 中文解释说明占位表（待人工填写）
///
/// 格式约定：`{一句话解释该按钮的作用}`。当前为占位，请替换为实际说明。
const DESC_ZH: &[(&str, &str)] = &[
    ("SkipBackward", "快退到起点"),
    ("Play", "开始播放"),
    ("Pause", "暂停播放"),
    ("SkipForward", "快进到末尾"),
    ("Record", "开始录制 MIDI 输入"),
    ("RecordStop", "停止录制"),
    ("Pointer", "选择/移动音符"),
    ("Pencil", "绘制音符"),
    ("Eraser", "擦除音符"),
    ("Curve", "绘制自动化曲线"),
    ("Quantize", "将选中音符量化到网格"),
    ("Speed", "变速（按ctrl点击打开面板）"),
    ("FlipVertical", "垂直翻转选中音符"),
    ("FlipHorizontal", "水平翻转选中音符"),
    ("Split", "分割音符（对齐演奏指示线分割）"),
    ("Glue", "合并选中的相邻音符"),
    ("Tie", "连奏选中音符"),
    ("TransposeDown", "下移选中音符音高（按住ctrl进行八度移动）"),
    ("TransposeUp", "上移选中音符音高（按住ctrl进行八度移动）"),
    ("Undo", "撤销上一步操作"),
    ("Redo", "重做已撤销操作"),
    ("Loop", "打开循环播放区域"),
    ("AutoScroll", "切换自动滚动模式"),
    ("Collaboration", "打开多人协作面板"),
    ("More", "打开更多工具菜单"),
];

/// 英文解释说明占位表（待人工填写）
///
/// 格式约定：`{one-line explanation}`。当前为占位，请替换为实际说明。
/// 根据按钮角色与语言，返回 `(按钮名, 解释说明占位)`。
///
/// 解释说明当前为 `{...}` 占位，待人工填写。

const DESC_EN: &[(&str, &str)] = &[
    ("SkipBackward", "Seek to start"),
    ("Play", "Start playback"),
    ("Pause", "Pause playback"),
    ("SkipForward", "Seek to end"),
    ("Record", "Start MIDI recording"),
    ("RecordStop", "Stop recording"),
    ("Pointer", "Select/move notes"),
    ("Pencil", "Draw notes"),
    ("Eraser", "Erase notes"),
    ("Curve", "Draw velocity curve"),
    ("Quantize", "Quantize selected notes to grid"),
    ("Speed", "Change speed (Ctrl+click to open panel)"),
    ("FlipVertical", "Flip selected notes vertically"),
    ("FlipHorizontal", "Flip selected notes horizontally"),
    ("Split", "Split notes at playhead"),
    ("Glue", "Glue selected adjacent notes"),
    ("Tie", "Tie selected notes"),
    ("TransposeDown", "Transpose selected notes down (Ctrl for octave)"),
    ("TransposeUp", "Transpose selected notes up (Ctrl for octave)"),
    ("Undo", "Undo last action"),
    ("Redo", "Redo undone action"),
    ("Loop", "Toggle loop region"),
    ("AutoScroll", "Toggle auto-scroll"),
    ("Collaboration", "Open realtime collaboration panel"),
    ("More", "Open more tools menu"),
];


pub fn button_desc(id: ButtonId, lang: Language) -> (&'static str, &'static str) {
    let name = button_name(id, main_translations(lang));
    let table = match lang {
        Language::ZhCn => DESC_ZH,
        Language::EnUs => DESC_EN,
    };
    let desc = table
        .iter()
        .find(|(key, _)| *key == id.as_str())
        .map(|(_, d)| *d)
        .unwrap_or("未配置说明");
    (name, desc)
}

impl ButtonId {
    /// 角色标识的字符串键（用于匹配描述表）
    pub fn as_str(self) -> &'static str {
        match self {
            ButtonId::SkipBackward => "SkipBackward",
            ButtonId::Play => "Play",
            ButtonId::Pause => "Pause",
            ButtonId::SkipForward => "SkipForward",
            ButtonId::Record => "Record",
            ButtonId::RecordStop => "RecordStop",
            ButtonId::Pointer => "Pointer",
            ButtonId::Pencil => "Pencil",
            ButtonId::Eraser => "Eraser",
            ButtonId::Curve => "Curve",
            ButtonId::Quantize => "Quantize",
            ButtonId::Speed => "Speed",
            ButtonId::FlipVertical => "FlipVertical",
            ButtonId::FlipHorizontal => "FlipHorizontal",
            ButtonId::Split => "Split",
            ButtonId::Glue => "Glue",
            ButtonId::Tie => "Tie",
            ButtonId::TransposeDown => "TransposeDown",
            ButtonId::TransposeUp => "TransposeUp",
            ButtonId::Undo => "Undo",
            ButtonId::Redo => "Redo",
            ButtonId::Loop => "Loop",
            ButtonId::AutoScroll => "AutoScroll",
            ButtonId::Collaboration => "Collaboration",
            ButtonId::More => "More",
        }
    }
}
