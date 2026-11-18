//! UI-023 收尾：预览 RGBA 缓存与参数 Reset 的回归。
//!
//! 两条都是「预览 / 面板」层的收尾项：
//! - 预览每帧被 egui 取用，必须按签名缓存，不能每帧重栅格化 + 复制缓冲；
//! - Reset 恢复默认排版参数，但保留用户选的字体与排版模式。

use lumino_core::{TextLayoutMode, TextParams};

use crate::Editor;

fn editor_with_text(text: &str) -> Editor {
    let mut editor = Editor::new();
    let mut state = lumino_editor_state::text_tool::TextToolState::new();
    state.set_drag(0.0, 3840.0, 60, 64);
    state.active = true;
    state.text = text.to_string();
    editor.editor_state.text_tool = state;
    editor
}

#[test]
fn test_preview_rgba_cache_hits_and_invalidates() {
    let mut editor = editor_with_text("A");
    let Some((w1, _h1, first)) = editor.text_tool_preview_rgba() else {
        eprintln!("跳过 test_preview_rgba_cache_hits_and_invalidates：无可用系统字体");
        return;
    };
    let (w2, _h2, second) = editor.text_tool_preview_rgba().expect("二次调用应仍有预览");
    assert_eq!(w1, w2, "同签名应给出同一尺寸");
    assert!(
        std::sync::Arc::ptr_eq(&first, &second),
        "同签名必须复用缓存，不得重新栅格化"
    );

    editor.editor_state.text_tool.text = "B".to_string();
    let Some((_, _, third)) = editor.text_tool_preview_rgba() else {
        return;
    };
    assert!(
        !std::sync::Arc::ptr_eq(&first, &third),
        "文本变化必须重建 RGBA"
    );

    editor.cancel_text_tool();
    assert!(
        editor.text_preview_cache.borrow().is_none(),
        "取消后应释放预览缓存"
    );
}

#[test]
fn test_reset_text_params_keeps_font_and_mode() {
    let mut editor = Editor::new();
    editor.editor_state.text_tool.params = TextParams {
        font: "Custom Font".to_string(),
        size: 48.0,
        weight: 700,
        italic: true,
        threshold: 80.0,
        tracking: 3.0,
        ..TextParams::default()
    };
    editor.editor_state.text_tool.layout_mode = TextLayoutMode::LegacyBox;

    editor.reset_text_params();

    let params = &editor.editor_state.text_tool.params;
    let defaults = TextParams::default();
    assert_eq!(params.font, "Custom Font", "Reset 不应换掉用户选的字体");
    assert_eq!(params.size, defaults.size, "Reset 应恢复默认字号");
    assert_eq!(params.weight, defaults.weight, "Reset 应恢复默认字重");
    assert!(!params.italic, "Reset 应关闭斜体");
    assert_eq!(params.threshold, defaults.threshold);
    assert_eq!(params.tracking, defaults.tracking);
    assert_eq!(
        editor.editor_state.text_tool.layout_mode,
        TextLayoutMode::LegacyBox,
        "Reset 不应改排版模式"
    );
}
