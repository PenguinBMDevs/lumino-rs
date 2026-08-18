//! Editor 核心方法 —— 单元测试
//!
//! 从 `impls/editor_impl.rs` 抽出，控制文件行数并保持单一职责。

use super::*;

#[test]
fn test_ctrl_pressed_defaults_false() {
    // 可靠通道（窗口级 CtrlKeyChanged）默认未按下，与 canvas 内状态互相兜底
    let editor = Editor::new();
    assert!(!editor.ctrl_pressed());
}

#[test]
fn test_ctrl_pressed_set_and_get() {
    let mut editor = Editor::new();
    editor.set_ctrl_pressed(true);
    assert!(editor.ctrl_pressed());
    editor.set_ctrl_pressed(false);
    assert!(!editor.ctrl_pressed());
}
