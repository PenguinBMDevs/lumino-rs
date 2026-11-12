//! 图片转 MIDI 按钮的缓存命中路径（DRAW-030）
//!
//! 只测「点击 → 复用缓存 / 重新转换」的分支选择，不等待后台线程结果。

use crate::right_sidebar::{I2mCacheEntry, I2mCacheKey};
use crate::root::Root;
use lumino_core::storage::config::UiConfig;
use lumino_editor_state::{ImageToMidiMode, ImageToMidiPreview, PreviewNote};
use lumino_message::RightSidebarAction;

fn create_root() -> Root {
    Root::new(&UiConfig::default())
}

/// 写一张占位图片文件（命中缓存时不会被解码）
fn fake_image(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(name);
    std::fs::write(&path, b"not-a-real-image").expect("写临时文件失败");
    path
}

fn sample_preview() -> ImageToMidiPreview {
    ImageToMidiPreview {
        tracks: vec![vec![PreviewNote {
            tick: 0.0,
            length: 4.0,
            key: 60,
        }]],
        orig_width: 4.0,
    }
}

#[test]
fn test_convert_click_reuses_cache_without_spawning() {
    let mut root = create_root();
    let path = fake_image("lumino_i2m_click_hit.png");
    root.right_sidebar.selected_image_path = Some(path.clone());

    let key = I2mCacheKey::new(&path, root.right_sidebar.config.to_settings()).expect("键应生成");
    root.right_sidebar.i2m_cache = Some(I2mCacheEntry {
        key,
        preview: sample_preview(),
    });
    let previous_tool = root.toolbar.current_tool;

    let handled = root.handle_right_sidebar_action(RightSidebarAction::ConvertClicked);
    assert!(handled);
    assert!(root.pending_i2m.is_none(), "命中缓存不应启动后台转换线程");
    assert!(!root.right_sidebar.converting);
    assert_eq!(
        root.editor.editor_state.image_to_midi.mode,
        ImageToMidiMode::Selecting,
        "命中缓存应直接进入框选阶段"
    );
    assert!(root.editor.editor_state.image_to_midi.preview.is_some());
    assert_eq!(
        root.toolbar.current_tool,
        crate::toolbar::Tool::PointerYSelect,
        "应切到 Y 向选择工具"
    );
    assert_eq!(
        root.i2m_restore_tool,
        Some(previous_tool),
        "√ 写入后要还原到转换前的工具"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_convert_click_reconverts_when_settings_change() {
    let mut root = create_root();
    let path = fake_image("lumino_i2m_click_miss.png");
    root.right_sidebar.selected_image_path = Some(path.clone());

    // 缓存里是「亮度 +10」的结果，面板是默认参数 → 必须重新转换
    let mut stale = root.right_sidebar.config.to_settings();
    stale.brightness = 10;
    let key = I2mCacheKey::new(&path, stale).expect("键应生成");
    root.right_sidebar.i2m_cache = Some(I2mCacheEntry {
        key,
        preview: sample_preview(),
    });

    let handled = root.handle_right_sidebar_action(RightSidebarAction::ConvertClicked);
    assert!(handled);
    assert!(root.right_sidebar.converting, "参数变化应重新转换");
    assert!(root.pending_i2m.is_some(), "应启动后台转换线程");

    // 丢弃接收端，后台线程的失败结果不会影响测试
    root.pending_i2m = None;
    let _ = std::fs::remove_file(&path);
}
