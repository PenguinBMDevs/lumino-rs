//! 图片转 MIDI 放置模式交互测试
//!
//! - 增量式拉伸（StretchLeft/StretchRight）：右/左边界以按下锚点为基准累加
//!   snap 增量，同 snap 网格内拖动不塌缩、不跳变（回归：旧实现全局 snap
//!   直接赋值导致窄素材拉伸无变化 / 瞬间塌缩）；
//! - clamp 最小宽度保护后回拖可恢复；
//! - 命中测试仅接受卷帘内容区（键盘列/标尺上方不可交互）；
//! - 区域框数据允许 Y 向越界（绘制层负责裁剪，数据不裁剪）。

use super::*;
use lumino_editor_state::{ImageToMidiPreview, PreviewNote};

/// 构造放置模式编辑器：素材预览 + 确认区域 (5000, 5300)
///
/// 素材：单轨、orig_width 300、音符 (0, 60, 100)。
/// 交互路径只依赖 `image_to_midi` 状态与视图参数，无需 document 种子。
fn placed_editor() -> Editor {
    let mut editor = Editor::new();
    let i2m = &mut editor.editor_state.image_to_midi;
    i2m.preview = Some(ImageToMidiPreview {
        tracks: vec![vec![PreviewNote {
            tick: 0.0,
            length: 100.0,
            key: 60,
        }]],
        orig_width: 300.0,
    });
    i2m.confirm_region(RegionRect::new(5000.0, 5300.0, 0, 127));
    // 画布尺寸（内容区/按钮钳制计算需要）
    editor.editor_state.canvas.size_x = 800.0;
    editor.editor_state.canvas.size_y = 600.0;
    editor
}

#[test]
fn test_stretch_right_is_incremental() {
    // 验证增量式拉伸：右边界以按下锚点为基准累加 snap 增量，
    // 同 snap 网格内拖动不塌缩、不跳变
    let mut editor = placed_editor();
    let view = &editor.editor_state.view;
    // 默认 snap_precision = 1920
    assert_eq!(view.snap_precision, 1920.0);

    // 按下区域框右边缘（x ≈ tick 5295，snap 后 3840）
    let right_x = view.tick_to_x(5300.0);
    let mid_y = view.key_to_y(60);
    editor.handle_i2m_pressed(
        Point::new(right_x - 1.0, mid_y),
        editor.snap_tick(5295.0),
        60.0,
    );
    assert_eq!(
        editor.editor_state.image_to_midi.interaction,
        I2mInteraction::StretchRight
    );

    // 拖到 tick 5900（snap 5760）：delta = +1920 → 右边界 5300 → 7220
    editor.handle_i2m_moved(5760.0, 60.0);
    let region = editor.editor_state.image_to_midi.region.expect("区域存在");
    assert_eq!(region.tick_end, 7220.0, "增量式拉伸应相对锚点累加");
    // 预览音符长度等比变化：100 * 2220/300 = 740
    let notes = editor.editor_state.image_to_midi.track_screen_notes(0);
    assert_eq!(notes[0].2, 740.0, "音符长度应随区域宽度等比变化");

    // 同 snap 网格内继续拖动（tick 6000-7600 都 snap 到 5760）：长度保持不变（预期吸附行为）
    editor.handle_i2m_moved(5760.0, 60.0);
    let region = editor.editor_state.image_to_midi.region.expect("区域存在");
    assert_eq!(region.tick_end, 7220.0);

    // 跨过下一网格（tick 8000 → snap 7680）：右边界 7220 → 9140
    editor.handle_i2m_moved(7680.0, 60.0);
    let region = editor.editor_state.image_to_midi.region.expect("区域存在");
    assert_eq!(region.tick_end, 9140.0);
}

#[test]
fn test_stretch_right_no_snap_collapse() {
    // 回归验证：旧实现把全局 snap 值直接赋给右边界，
    // 素材窄于 snap 精度时（300 < 1920）拖到 5000-6900 区间会瞬间塌缩
    // 到 tick_start + 1（宽度 1 tick，音符全部压成 1 tick）。
    let mut editor = placed_editor();
    let view = &editor.editor_state.view;
    editor.handle_i2m_pressed(
        Point::new(view.tick_to_x(5300.0) - 1.0, view.key_to_y(60)),
        editor.snap_tick(5295.0),
        60.0,
    );
    // 鼠标拖到 tick 5600（snap 3840，等于按下锚点）→ 增量 delta=0，右边界不塌缩
    editor.handle_i2m_moved(3840.0, 60.0);
    let region = editor.editor_state.image_to_midi.region.expect("区域存在");
    assert_eq!(region.tick_end, 5300.0, "同 snap 网格内不得塌缩");
}

#[test]
fn test_stretch_left_is_incremental() {
    let mut editor = placed_editor();
    let view = &editor.editor_state.view;
    // 按下区域框左边缘（x ≈ tick 5005，snap 后 3840）
    editor.handle_i2m_pressed(
        Point::new(view.tick_to_x(5000.0) + 1.0, view.key_to_y(60)),
        editor.snap_tick(5005.0),
        60.0,
    );
    assert_eq!(
        editor.editor_state.image_to_midi.interaction,
        I2mInteraction::StretchLeft
    );

    // 向左拖（tick 3100 → snap 1920）：delta = -1920 → 左边界 5000 → 3080
    editor.handle_i2m_moved(1920.0, 60.0);
    let region = editor.editor_state.image_to_midi.region.expect("区域存在");
    assert_eq!(region.tick_start, 3080.0);
    // 右边界保持 5300 → 宽度 2220
    assert_eq!(region.tick_end, 5300.0);
    // 音符起点 tick 随区域左移：0/300*2220 + 3080 = 3080
    let notes = editor.editor_state.image_to_midi.track_screen_notes(0);
    assert_eq!(notes[0].0, 3080.0);
}

#[test]
fn test_stretch_right_recovers_after_min_clamp() {
    // clamp 保护：左拖到最小宽度（tick_start + 1）后，右拖应能恢复（不卡死）
    let mut editor = placed_editor();
    let view = &editor.editor_state.view;
    editor.handle_i2m_pressed(
        Point::new(view.tick_to_x(5300.0) - 1.0, view.key_to_y(60)),
        editor.snap_tick(5295.0),
        60.0,
    );
    // 大幅左拖：delta = -3840 → 右边界 5300 → clamp 到最小宽度 5001
    editor.handle_i2m_moved(0.0, 60.0);
    let region = editor.editor_state.image_to_midi.region.expect("区域存在");
    assert_eq!(region.tick_end, 5001.0, "最小宽度保护：tick_start + 1");
    // 同 snap 网格内继续拖动：右边界保持
    editor.handle_i2m_moved(0.0, 60.0);
    let region = editor.editor_state.image_to_midi.region.expect("区域存在");
    assert_eq!(region.tick_end, 5001.0);
    // 右拖回（相对 clamp 后锚点 0，delta = +5760）：右边界恢复，不卡死
    editor.handle_i2m_moved(5760.0, 60.0);
    let region = editor.editor_state.image_to_midi.region.expect("区域存在");
    assert_eq!(region.tick_end, 10761.0, "clamp 后右拖应能恢复");
}

#[test]
fn test_hit_test_i2m_region_ignores_outside_content() {
    let editor = placed_editor();
    let view = &editor.editor_state.view;
    // 键盘列内点击（x < keyboard_width）：即使选框越界也不可命中
    assert!(
        editor
            .hit_test_i2m_region(Point::new(50.0, view.key_to_y(60)))
            .is_none()
    );
    // 标尺内点击（y < ruler_height）：不可命中
    assert!(
        editor
            .hit_test_i2m_region(Point::new(700.0, 10.0))
            .is_none()
    );
    // 内容区内、选框内部：可命中
    assert_eq!(
        editor.hit_test_i2m_region(Point::new(view.tick_to_x(5100.0), view.key_to_y(60))),
        Some(SelectionHitType::Inside)
    );
}

#[test]
fn test_region_screen_bounds_with_wrapped_key() {
    // 素材 Y 向越界（key_hi 回绕成 255）时，屏幕边界允许越出内容区；
    // 裁剪由绘制层负责（clip_region_bounds），数据本身不裁剪
    let mut editor = placed_editor();
    editor.editor_state.image_to_midi.allow_y_drag = true;
    // 模拟持续上移导致 key_hi 回绕
    let region = editor
        .editor_state
        .image_to_midi
        .region
        .as_mut()
        .expect("区域存在");
    region.key_hi = 255;
    let (left, right, top, _) = editor.i2m_region_screen_bounds().expect("应有边界");
    assert_eq!(left, editor.editor_state.view.tick_to_x(5000.0));
    assert_eq!(right, editor.editor_state.view.tick_to_x(5300.0));
    assert!(
        top < editor.editor_state.view.ruler_height,
        "越界顶边应超出标尺（由绘制层裁剪）"
    );
}
