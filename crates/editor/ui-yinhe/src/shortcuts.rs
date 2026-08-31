//! 快捷键映射 — yinhe 25 action → lumino `EditorAction` / `ToolbarEvent` 复用罗盘
//!
//! 约束（P7）：
//! - 数据模型直接用 Lumino 工程格式，混音台不迁，多文档标签不做
//! - i18n 按 lumino（`lumino_extras::i18n`），快捷键要统一
//! - 发布形态为可选编译 `--features yinhe`（本文件无条件编译，跟随 `lumino-ui-yinhe` crate）
//! - **复用罗盘**：不新定义 `Keybindings` 持久化结构，不落盘，不污染 `UiConfig` / `UiState`，
//!   仅提供 `KeyCode + modifiers → Message` 的纯函数映射，复用 `Host::handle_keyboard_shortcuts`
//!   的罗盘语义（`is_ctrl_or_cmd_pressed` 统一处理 `Ctrl`/`Cmd`）。
//! - 存储：`YinheState` / `YinheLayout` 独立 `yinhe_layout.json`，见 `crate::state`，本模块不涉及 IO。
//!
//! 25 actions 的选取：对齐 yinhe 原 `platform::MenuAction` + `chrome::transport` + `toolbar` 常用动作，
//! 在 lumino 侧均有现有 `EditorAction` / `ToolbarEvent` / `Message` 可直接复用，
//! 不新增消息变体，避免跨 crate 消息膨胀。

use lumino_ui_core::Message as LuminoMessage;
use lumino_ui_core::toolbar_event::{Event as ToolbarEvent, FlipHorizontalMode};
use lumino_message::Tool;
use winit::keyboard::KeyCode;

/// Yinhe 侧 25 个可快捷键动作（对齐 yinhe 原 25 keybindings）
///
/// 每个变体均可无损映射到现有 Lumino `Message`（`EditorAction` / `ToolbarEvent` / `Arrangement*`），
/// 不新增消息类型，复用罗盘。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum YinheAction {
    /// 撤销 — `Ctrl+Z` → `EditorAction::Undo`
    Undo,
    /// 重做 — `Ctrl+Y` / `Ctrl+Shift+Z` → `EditorAction::Redo`
    Redo,
    /// 剪切 — `Ctrl+X` → `EditorAction::Cut`
    Cut,
    /// 复制 — `Ctrl+C` → `EditorAction::Copy`
    Copy,
    /// 粘贴 — `Ctrl+V` → `EditorAction::Paste`
    Paste,
    /// 全选 — `Ctrl+A` → `EditorAction::SelectAll`
    SelectAll,
    /// 删除 — `Delete` / `Backspace` → `EditorAction::DeletePressed`
    Delete,
    /// 量化 — `Ctrl+Q` → `ToolbarEvent::Quantize`
    Quantize,
    /// 播放 / 暂停切换 — `Space` → `ToolbarEvent::Play` / `Pause`（由 Host 根据 `is_playing` 决策）
    TogglePlay,
    /// 停止 — `Esc` → `ToolbarEvent::Stop`
    Stop,
    /// 循环切换 — `L` → `ToolbarEvent::ToggleLoop`
    ToggleLoop,
    /// 录制切换 — `Ctrl+R` → `ToolbarEvent::Record` / `RecordStop`
    ToggleRecord,
    /// 上移调 — `Ctrl+Up` → `ToolbarEvent::TransposeUp(1)`
    TransposeUp,
    /// 下移调 — `Ctrl+Down` → `ToolbarEvent::TransposeDown(1)`
    TransposeDown,
    /// 垂直翻转 — `Ctrl+Shift+V` → `ToolbarEvent::FlipVertical`
    FlipVertical,
    /// 水平翻转（居中）— `Ctrl+Shift+H` → `ToolbarEvent::FlipHorizontal(Center)`
    FlipHorizontal,
    /// 音符变速 — `Ctrl+Shift+S` → `ToolbarEvent::SpeedChange`
    SpeedChange,
    /// 分割 — `Ctrl+K` → `ToolbarEvent::Split`
    Split,
    /// 合并 — `Ctrl+G` → `ToolbarEvent::Glue`
    Glue,
    /// 连奏 — `Ctrl+T` → `ToolbarEvent::Tie`
    Tie,
    /// 选择工具（指针） — `V` → `ToolbarEvent::ToolSelected(Pointer)`
    ToolPointer,
    /// 铅笔工具 — `P` → `ToolbarEvent::ToolSelected(Pencil)`
    ToolPencil,
    /// 橡皮擦 — `E` → `ToolbarEvent::ToolSelected(Eraser)`
    ToolEraser,
    /// 刷子 — `B` → `ToolbarEvent::ToolSelected(Brush)`
    ToolBrush,
    /// 曲线 — `C` → `ToolbarEvent::ToolSelected(Curve)`
    ToolCurve,
}

impl YinheAction {
    /// 所有 25 actions（供设置页/测试遍历，不落盘）
    pub const ALL: [Self; 25] = [
        Self::Undo,
        Self::Redo,
        Self::Cut,
        Self::Copy,
        Self::Paste,
        Self::SelectAll,
        Self::Delete,
        Self::Quantize,
        Self::TogglePlay,
        Self::Stop,
        Self::ToggleLoop,
        Self::ToggleRecord,
        Self::TransposeUp,
        Self::TransposeDown,
        Self::FlipVertical,
        Self::FlipHorizontal,
        Self::SpeedChange,
        Self::Split,
        Self::Glue,
        Self::Tie,
        Self::ToolPointer,
        Self::ToolPencil,
        Self::ToolEraser,
        Self::ToolBrush,
        Self::ToolCurve,
    ];

    /// 展示名（i18n key 回落英文，保持与 lumino `toolbar` 按钮文案一致）
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Undo => "撤销",
            Self::Redo => "重做",
            Self::Cut => "剪切",
            Self::Copy => "复制",
            Self::Paste => "粘贴",
            Self::SelectAll => "全选",
            Self::Delete => "删除",
            Self::Quantize => "量化",
            Self::TogglePlay => "播放/暂停",
            Self::Stop => "停止",
            Self::ToggleLoop => "循环",
            Self::ToggleRecord => "录制",
            Self::TransposeUp => "移调 +1",
            Self::TransposeDown => "移调 -1",
            Self::FlipVertical => "垂直翻转",
            Self::FlipHorizontal => "水平翻转",
            Self::SpeedChange => "变速",
            Self::Split => "分割",
            Self::Glue => "合并",
            Self::Tie => "连奏",
            Self::ToolPointer => "选择工具",
            Self::ToolPencil => "铅笔",
            Self::ToolEraser => "橡皮擦",
            Self::ToolBrush => "刷子",
            Self::ToolCurve => "曲线",
        }
    }

    /// 默认快捷键展示（与 `match_yinhe_shortcut` 一致，供设置页 placeholder）
    pub fn default_shortcut(self) -> &'static str {
        match self {
            Self::Undo => "Ctrl+Z",
            Self::Redo => "Ctrl+Y",
            Self::Cut => "Ctrl+X",
            Self::Copy => "Ctrl+C",
            Self::Paste => "Ctrl+V",
            Self::SelectAll => "Ctrl+A",
            Self::Delete => "Delete",
            Self::Quantize => "Ctrl+Q",
            Self::TogglePlay => "Space",
            Self::Stop => "Esc",
            Self::ToggleLoop => "L",
            Self::ToggleRecord => "Ctrl+R",
            Self::TransposeUp => "Ctrl+↑",
            Self::TransposeDown => "Ctrl+↓",
            Self::FlipVertical => "Ctrl+Shift+V",
            Self::FlipHorizontal => "Ctrl+Shift+H",
            Self::SpeedChange => "Ctrl+Shift+S",
            Self::Split => "Ctrl+K",
            Self::Glue => "Ctrl+G",
            Self::Tie => "Ctrl+T",
            Self::ToolPointer => "V",
            Self::ToolPencil => "P",
            Self::ToolEraser => "E",
            Self::ToolBrush => "B",
            Self::ToolCurve => "C",
        }
    }
}

/// 将 `YinheAction` 映射到现有 Lumino `Message`（复用罗盘，不新定义消息）
///
/// - 编辑类 → `Message::EditorAction`
/// - 工具/走带类 → `Message::Toolbar`
/// - 播放类（`TogglePlay` / `Stop`）由调用方按 `is_playing` 决定 `Play`/`Pause`/`Stop`，
///   此处 `TogglePlay` 暂映射为 `ToolbarEvent::Play`（与 `Host::handle_space_shortcut`
///   的切换语义等价，实际 Host 侧会按状态二次路由），`Stop` 映射为 `ToolbarEvent::Stop`。
/// - `Ctrl` 已在外层通过 `is_ctrl_or_cmd_pressed` 统一（macOS `Cmd` 等价 `Ctrl`），
///   本函数仅做纯映射，无持久化。
pub fn yinhe_action_to_message(action: YinheAction, is_playing: bool) -> Option<LuminoMessage> {
    use lumino_message::EditorAction;
    let msg = match action {
        YinheAction::Undo => LuminoMessage::EditorAction(EditorAction::Undo),
        YinheAction::Redo => LuminoMessage::EditorAction(EditorAction::Redo),
        YinheAction::Cut => LuminoMessage::EditorAction(EditorAction::Cut),
        YinheAction::Copy => LuminoMessage::EditorAction(EditorAction::Copy),
        YinheAction::Paste => LuminoMessage::EditorAction(EditorAction::Paste),
        YinheAction::SelectAll => LuminoMessage::EditorAction(EditorAction::SelectAll),
        YinheAction::Delete => LuminoMessage::EditorAction(EditorAction::DeletePressed),
        YinheAction::Quantize => LuminoMessage::Toolbar(ToolbarEvent::Quantize),
        YinheAction::TogglePlay => {
            if is_playing {
                LuminoMessage::Toolbar(ToolbarEvent::Pause)
            } else {
                LuminoMessage::Toolbar(ToolbarEvent::Play)
            }
        }
        YinheAction::Stop => LuminoMessage::Toolbar(ToolbarEvent::Stop),
        YinheAction::ToggleLoop => LuminoMessage::Toolbar(ToolbarEvent::ToggleLoop),
        YinheAction::ToggleRecord => {
            if is_playing {
                // 录制中按切换 → 停止录制（与 Host 侧 `RecordStop` 对齐）
                LuminoMessage::Toolbar(ToolbarEvent::RecordStop)
            } else {
                LuminoMessage::Toolbar(ToolbarEvent::Record)
            }
        }
        YinheAction::TransposeUp => LuminoMessage::Toolbar(ToolbarEvent::TransposeUp(1)),
        YinheAction::TransposeDown => LuminoMessage::Toolbar(ToolbarEvent::TransposeDown(1)),
        YinheAction::FlipVertical => LuminoMessage::Toolbar(ToolbarEvent::FlipVertical),
        YinheAction::FlipHorizontal => {
            LuminoMessage::Toolbar(ToolbarEvent::FlipHorizontal(FlipHorizontalMode::Center))
        }
        YinheAction::SpeedChange => LuminoMessage::Toolbar(ToolbarEvent::SpeedChange),
        YinheAction::Split => LuminoMessage::Toolbar(ToolbarEvent::Split),
        YinheAction::Glue => LuminoMessage::Toolbar(ToolbarEvent::Glue),
        YinheAction::Tie => LuminoMessage::Toolbar(ToolbarEvent::Tie),
        YinheAction::ToolPointer => LuminoMessage::Toolbar(ToolbarEvent::ToolSelected(Tool::Pointer)),
        YinheAction::ToolPencil => LuminoMessage::Toolbar(ToolbarEvent::ToolSelected(Tool::Pencil)),
        YinheAction::ToolEraser => LuminoMessage::Toolbar(ToolbarEvent::ToolSelected(Tool::Eraser)),
        YinheAction::ToolBrush => LuminoMessage::Toolbar(ToolbarEvent::ToolSelected(Tool::Brush)),
        YinheAction::ToolCurve => LuminoMessage::Toolbar(ToolbarEvent::ToolSelected(Tool::Curve)),
    };
    Some(msg)
}

/// 匹配 Yinhe 快捷键：`KeyCode + ctrl/shift → YinheAction`
///
/// 复用罗盘：`ctrl` 已由外层 `is_ctrl_or_cmd_pressed` 统一（含 macOS `Cmd`），
/// 本函数不再区分 `Ctrl`/`Cmd`，与 `Host::match_editor_shortcut` 语义一致。
/// 不做落盘，纯函数映射（25 actions）。
pub fn match_yinhe_shortcut(
    key: KeyCode,
    ctrl: bool,
    shift: bool,
) -> Option<YinheAction> {
    match (key, ctrl, shift) {
        // 编辑基础（与 Host::match_editor_shortcut 对齐，复用罗盘）
        (KeyCode::KeyZ, true, false) => Some(YinheAction::Undo),
        (KeyCode::KeyY, true, _) => Some(YinheAction::Redo),
        (KeyCode::KeyZ, true, true) => Some(YinheAction::Redo),
        (KeyCode::KeyX, true, false) => Some(YinheAction::Cut),
        (KeyCode::KeyC, true, false) => Some(YinheAction::Copy),
        (KeyCode::KeyV, true, false) => Some(YinheAction::Paste),
        (KeyCode::KeyA, true, false) => Some(YinheAction::SelectAll),
        (KeyCode::Delete | KeyCode::Backspace, _, _) => Some(YinheAction::Delete),
        // 量化（复用 Host Ctrl+Q）
        (KeyCode::KeyQ, true, false) => Some(YinheAction::Quantize),
        // 播放控制
        (KeyCode::Space, _, _) => Some(YinheAction::TogglePlay),
        (KeyCode::Escape, _, _) => Some(YinheAction::Stop),
        (KeyCode::KeyL, false, false) => Some(YinheAction::ToggleLoop),
        (KeyCode::KeyR, true, false) => Some(YinheAction::ToggleRecord),
        // 音高/编辑（工具栏走带）
        (KeyCode::ArrowUp, true, false) => Some(YinheAction::TransposeUp),
        (KeyCode::ArrowDown, true, false) => Some(YinheAction::TransposeDown),
        (KeyCode::KeyV, true, true) => Some(YinheAction::FlipVertical),
        (KeyCode::KeyH, true, true) => Some(YinheAction::FlipHorizontal),
        (KeyCode::KeyS, true, true) => Some(YinheAction::SpeedChange),
        (KeyCode::KeyK, true, false) => Some(YinheAction::Split),
        (KeyCode::KeyG, true, false) => Some(YinheAction::Glue),
        (KeyCode::KeyT, true, false) => Some(YinheAction::Tie),
        // 工具切换（无修饰，直按字母，复用罗盘：与 toolbar tool 切换一致）
        (KeyCode::KeyV, false, false) => Some(YinheAction::ToolPointer),
        (KeyCode::KeyP, false, false) => Some(YinheAction::ToolPencil),
        (KeyCode::KeyE, false, false) => Some(YinheAction::ToolEraser),
        (KeyCode::KeyB, false, false) => Some(YinheAction::ToolBrush),
        (KeyCode::KeyC, false, false) => Some(YinheAction::ToolCurve),
        _ => None,
    }
}

/// 快捷匹配 Yinhe 快捷键并直接映射到 `LuminoMessage`（供 `Host::handle_keyboard_shortcuts` 调用）
///
/// `is_playing` 影响 `TogglePlay` / `ToggleRecord` 的 `Play`↔`Pause` / `Record`↔`RecordStop` 切换。
pub fn yinhe_shortcut_to_message(
    key: KeyCode,
    ctrl: bool,
    shift: bool,
    is_playing: bool,
) -> Option<LuminoMessage> {
    let action = match_yinhe_shortcut(key, ctrl, shift)?;
    yinhe_action_to_message(action, is_playing)
}

/// 兼容旧 Host 罗盘：处理 Yinhe 专属快捷键，命中返回 `Some(Message)` 并由调用方 `route_message`
///
/// 非 Yinhe 命中返回 `None`，交由原 `Host::match_editor_shortcut` / `match_arrangement_shortcut` 继续。
/// `is_yinhe_mode` 由外层 `AppMode::Yinhe` 决定，仅在 Yinhe 模式下启用工具切换等独占绑定，
/// 避免与 Editor 模式的字母输入/其他绑定冲突。
pub fn try_match_yinhe_message(
    key: KeyCode,
    ctrl: bool,
    shift: bool,
    is_playing: bool,
    is_yinhe_mode: bool,
) -> Option<LuminoMessage> {
    // 工具类单字母绑定仅在 Yinhe 模式下生效，避免 Editor 模式误触
    let is_tool_key = matches!(
        key,
        KeyCode::KeyV | KeyCode::KeyP | KeyCode::KeyE | KeyCode::KeyB | KeyCode::KeyC
    ) && !ctrl
        && !shift;
    if is_tool_key && !is_yinhe_mode {
        return None;
    }
    yinhe_shortcut_to_message(key, ctrl, shift, is_playing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_actions_have_valid_display() {
        for a in YinheAction::ALL {
            assert!(!a.display_name().is_empty());
            assert!(!a.default_shortcut().is_empty());
        }
        assert_eq!(YinheAction::ALL.len(), 25);
    }

    #[test]
    fn match_undo_redo() {
        assert_eq!(
            match_yinhe_shortcut(KeyCode::KeyZ, true, false),
            Some(YinheAction::Undo)
        );
        assert_eq!(
            match_yinhe_shortcut(KeyCode::KeyY, true, false),
            Some(YinheAction::Redo)
        );
        assert_eq!(
            match_yinhe_shortcut(KeyCode::KeyZ, true, true),
            Some(YinheAction::Redo)
        );
    }

    #[test]
    fn action_to_message_uses_existing_lumino_variants() {
        // 编辑类复用 EditorAction
        let msg = yinhe_action_to_message(YinheAction::Copy, false).expect("copy");
        assert!(matches!(msg, LuminoMessage::EditorAction(_)));
        // 工具类复用 ToolbarEvent
        let msg = yinhe_action_to_message(YinheAction::Quantize, false).expect("quantize");
        assert!(matches!(msg, LuminoMessage::Toolbar(_)));
        // 播放切换按 is_playing 区分
        let play = yinhe_action_to_message(YinheAction::TogglePlay, false).expect("play");
        assert!(matches!(play, LuminoMessage::Toolbar(ToolbarEvent::Play)));
        let pause = yinhe_action_to_message(YinheAction::TogglePlay, true).expect("pause");
        assert!(matches!(pause, LuminoMessage::Toolbar(ToolbarEvent::Pause)));
    }

    #[test]
    fn tool_keys_only_in_yinhe_mode() {
        // 非 Yinhe 模式：V/P 等工具键不命中，交由 Editor 侧处理
        assert!(
            try_match_yinhe_message(KeyCode::KeyV, false, false, false, false).is_none()
        );
        // Yinhe 模式：命中
        assert!(try_match_yinhe_message(KeyCode::KeyV, false, false, false, true).is_some());
        // Ctrl 组合不受此限制（量化等）
        assert!(try_match_yinhe_message(KeyCode::KeyQ, true, false, false, false).is_some());
    }

    #[test]
    fn no_persistence_new_keybindings_file() {
        // 本模块不定义、不持久化 Keybindings（复用罗盘），仅纯函数映射
        // 通过此处断言：无文件 IO、无 serde 持久化类型
        // 若未来引入 Keybindings 持久化，此测试应失败提醒移除
        assert_eq!(YinheAction::ALL.len(), 25);
    }
}
