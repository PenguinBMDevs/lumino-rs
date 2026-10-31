//! CPU 洋葱皮帧生成验收：跳过当前轨/静音轨、帧矩形 = 网格内容区、帧冻结。
//!
//! 这里直接驱动 [`CpuOnionState`]（Host 侧只是收集相机/静音并转发），
//! 避免构造需要窗口与 GPU 设备的 `Host`——被测逻辑与被测输入完全一致。

use std::sync::Arc;

use lumino_gfx::CpuOnionFrame;
use lumino_midi_model::{MidiDocument, NoteEvent};

use super::camera::CpuOnionCameraInput;
use super::collect_muted_tracks;
use super::state::{CpuOnionFrameKey, CpuOnionState};
use crate::sidebar::Track;

/// 帧尺寸：640 × 240（zoom_y = 20 时可见 127..115 键，足够覆盖 120/121/122）。
const WIDTH: f32 = 640.0;
const HEIGHT: f32 = 240.0;

/// 1:1 DPI 的相机输入（无键盘栏/标尺时画布铺满帧）。
fn camera_input(scale_factor: f32) -> CpuOnionCameraInput {
    CpuOnionCameraInput {
        scale_factor,
        canvas_offset: (0.0, 0.0),
        canvas_size: (WIDTH, HEIGHT),
        keyboard_width: 0.0,
        ruler_height: 0.0,
        scroll: (0.0, 0.0),
        zoom: (1.0, 20.0),
        key_count: 128,
    }
}

/// 三轨文档：轨 0 = 当前轨，轨 1 = 洋葱皮轨，轨 2 = 静音轨，键位依次错开。
fn document() -> MidiDocument {
    let mut document = MidiDocument::empty_with_tracks(3, 480);
    document.insert_note(0, NoteEvent::new(10, 110, 120, 100, 0));
    document.insert_note(1, NoteEvent::new(10, 110, 121, 100, 0));
    document.insert_note(2, NoteEvent::new(10, 110, 122, 100, 0));
    document
}

/// 键位所在的像素行（取行中心，避开 2px 描边）。
fn note_row(key: u16) -> u32 {
    (127u16.saturating_sub(key) as f32 * 20.0 + 10.0) as u32
}

/// 取像素 alpha（越界返回 0，测试里当透明处理）。
fn alpha(frame: &CpuOnionFrame, x: u32, y: u32) -> u8 {
    let offset = ((y as usize) * frame.width as usize + x as usize) * 4;
    frame.pixels.get(offset + 3).copied().unwrap_or(0)
}

fn frame_key(geometry: super::camera::CpuOnionGeometry) -> CpuOnionFrameKey {
    CpuOnionFrameKey {
        track_notes_gen: 1,
        current_track: 0,
        mute_fingerprint: 7,
        palette_idx: 0,
        geometry,
    }
}

/// 当前轨/静音轨必须透明，非当前非静音轨 alpha = 255；无键盘栏/标尺时帧矩形 = 整块画布。
#[test]
fn cpu_frame_skips_current_and_muted_tracks() {
    let document = document();
    let muted = vec![false, false, true];
    let geometry = camera_input(1.0).to_physical().expect("100% 相机应可换算");
    let mut state = CpuOnionState::default();

    let output = state.update(frame_key(geometry), &document, &muted, None, None);
    assert!(output.rebuilt, "首次调用必须重新光栅化");
    let frame = output.frame.expect("有文档且尺寸合法时必须生成帧");

    // 无边栏：内容区 = 整块画布（与 render_pass 的 canvas scissor 同口径）
    assert_eq!((frame.x, frame.y), (0, 0));
    assert_eq!(frame.size(), (WIDTH as u32, HEIGHT as u32));
    assert!(frame.validate(), "帧像素长度必须与尺寸自洽");

    assert_eq!(
        alpha(&frame, 60, note_row(120)),
        0,
        "当前轨由 GPU 负责，CPU 帧必须透明"
    );
    assert_eq!(
        alpha(&frame, 60, note_row(122)),
        0,
        "静音轨必须透明（muted[2] = true）"
    );
    assert_eq!(
        alpha(&frame, 60, note_row(121)),
        255,
        "非当前、非静音的洋葱皮轨必须绘制"
    );
    assert_eq!(alpha(&frame, 0, 0), 0, "背景必须全透明");
}

/// 帧矩形必须是**网格内容区**（= canvas 去掉键盘栏/标尺），不是整块 canvas。
///
/// 回归防线：修复前帧矩形 = canvas（0,0,640,240），CPU 音符相对 GPU 网格整体
/// 左移 `keyboard_width`、上移 `ruler_height`（用户可见的「音符没对齐网格线」）。
#[test]
fn cpu_frame_rect_is_grid_content_area() {
    let document = document();
    let muted = vec![false, false, true];

    // 100%：内容区原点 = 边栏内缩量，尺寸 = 画布减内缩
    let mut input = camera_input(1.0);
    input.keyboard_width = 40.0;
    input.ruler_height = 16.0;
    let geometry = input.to_physical().expect("带边栏的相机应可换算");
    assert_eq!(
        (geometry.x, geometry.y),
        (40, 16),
        "帧原点必须是内容区原点（键盘栏/标尺右侧、下方）"
    );
    assert_eq!(
        (geometry.width, geometry.height),
        (600, 224),
        "帧尺寸必须是内容区尺寸（640−40 / 240−16）"
    );

    let mut state = CpuOnionState::default();
    let frame = state
        .update(frame_key(geometry), &document, &muted, None, None)
        .frame
        .expect("应生成帧");
    assert_eq!(
        (frame.x, frame.y),
        (40, 16),
        "CpuOnionFrame 必须落在内容区原点"
    );
    assert_eq!(frame.size(), (600, 224), "CpuOnionFrame 必须是内容区尺寸");
    assert_eq!(
        frame.size(),
        (geometry.camera.width, geometry.camera.height),
        "相机宽高必须与帧物理尺寸一致（局部坐标以内容区左上角为原点）"
    );
    assert!(frame.validate(), "帧像素长度必须与尺寸自洽");
    // 内容区局部行号不变：音符仍落在同一 key 行（只是整体随帧矩形右下移）
    assert_eq!(
        alpha(&frame, 60, note_row(121)),
        255,
        "内容区局部坐标里洋葱皮音符照常绘制"
    );

    // 125%：内容区四边全部 ×1.25 后 floor（41→51.25、17→21.25、599→748.75、223→278.75）
    let mut scaled = camera_input(1.25);
    scaled.keyboard_width = 41.0;
    scaled.ruler_height = 17.0;
    let scaled_geometry = scaled.to_physical().expect("125% 带边栏应可换算");
    assert_eq!((scaled_geometry.x, scaled_geometry.y), (51, 21));
    assert_eq!((scaled_geometry.width, scaled_geometry.height), (748, 278));
    let frame = CpuOnionState::default()
        .update(frame_key(scaled_geometry), &document, &muted, None, None)
        .frame
        .expect("125% 应生成帧");
    assert_eq!((frame.x, frame.y), (51, 21));
    assert_eq!(frame.size(), (748, 278));
}

/// 帧矩形随 DPI 换算成物理像素（125%：全部 ×1.25 并向下取整）。
#[test]
fn cpu_frame_rect_is_physical_pixels() {
    let document = document();
    let muted = vec![false, false, true];
    let mut input = camera_input(1.25);
    input.canvas_offset = (10.0, 5.0);
    input.canvas_size = (200.0, 100.0);
    let geometry = input.to_physical().expect("125% 相机应可换算");
    // 10 × 1.25 = 12.5 → 12；5 × 1.25 = 6.25 → 6；200 × 1.25 = 250；100 × 1.25 = 125
    assert_eq!((geometry.x, geometry.y), (12, 6));
    assert_eq!((geometry.width, geometry.height), (250, 125));

    let mut state = CpuOnionState::default();
    let output = state.update(frame_key(geometry), &document, &muted, None, None);
    let frame = output.frame.expect("应生成帧");
    assert_eq!((frame.x, frame.y), (12, 6), "帧矩形必须是物理像素");
    assert_eq!(frame.size(), (250, 125), "帧尺寸必须是物理 canvas 尺寸");
    assert_eq!(
        state.frame().map(CpuOnionFrame::size),
        Some((250, 125)),
        "状态里冻结的帧必须与返回帧一致"
    );
}

/// 浮点近似断言：×1.5 的 f32 舍入误差远小于 1e-4。
fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() < 1e-4,
        "{what} 期望 {expected}，实际 {actual}"
    );
}

/// 150% + 小数滚动/缩放：矩形 floor 成物理像素，相机滚动/缩放保留浮点（不取整）。
///
/// 与 GPU 主轨同口径：canvas scissor 用 `floor(逻辑值 × scale)`，而相机
/// `scroll` / `zoom` 是浮点直乘——先 floor 相机值会让 CPU 帧相对主轨产生
/// 亚像素错位，本用例把这两条口径同时锁住。
#[test]
fn cpu_frame_geometry_at_150_percent_with_fractional_values() {
    let document = document();
    let muted = vec![false, false, true];
    let mut input = camera_input(1.5);
    input.canvas_offset = (37.3, 21.9);
    input.canvas_size = (333.3, 200.1);
    input.scroll = (12.7, 3.3);
    input.zoom = (1.7, 20.3);
    let geometry = input.to_physical().expect("150% 小数输入应可换算");

    // 矩形：逻辑值 ×1.5 后向下取整（与 canvas scissor 同口径）。
    assert_eq!(
        (geometry.x, geometry.y),
        (55, 32),
        "37.3×1.5=55.95 → 55；21.9×1.5=32.85 → 32"
    );
    assert_eq!(
        (geometry.width, geometry.height),
        (499, 300),
        "333.3×1.5=499.95 → 499；200.1×1.5=300.15 → 300"
    );

    // 相机：滚动/缩放保留浮点，只乘 DPI（不得 floor）。
    assert_close(geometry.camera.scroll_x, 19.05, "scroll_x");
    assert_close(geometry.camera.scroll_y, 4.95, "scroll_y");
    assert_close(geometry.camera.zoom_x, 2.55, "zoom_x");
    assert_close(geometry.camera.zoom_y, 30.45, "zoom_y");
    assert_eq!(geometry.camera.key_count, 128);

    let mut state = CpuOnionState::default();
    let frame = state
        .update(frame_key(geometry), &document, &muted, None, None)
        .frame
        .expect("应生成帧");
    assert_eq!(
        (frame.x, frame.y),
        (55, 32),
        "CpuOnionFrame 物理矩形必须 floor（与 geometry 一致）"
    );
    assert_eq!(
        frame.size(),
        (499, 300),
        "CpuOnionFrame 尺寸必须 = 内容区物理尺寸（无边栏时即整块画布）"
    );
    assert_eq!(
        (geometry.camera.width, geometry.camera.height),
        frame.size(),
        "相机宽高必须与帧物理尺寸一致"
    );
    assert!(frame.validate(), "帧像素长度必须与尺寸自洽");
}

/// 指纹不变 → 复用上一帧（不重渲染、不递增版本、像素 Arc 相同）。
#[test]
fn cpu_frame_is_frozen_until_inputs_change() {
    let document = document();
    let muted = vec![false, false, true];
    let geometry = camera_input(1.0).to_physical().expect("100% 相机应可换算");
    let mut state = CpuOnionState::default();
    let key = frame_key(geometry);

    let first = state.update(key, &document, &muted, None, None);
    assert!(first.rebuilt);
    let first = first.frame.expect("首帧应生成");

    // 同一指纹重复调用：不得重渲染、不得重传（版本相同 + 像素共享）
    let second = state.update(key, &document, &muted, None, None);
    assert!(!second.rebuilt, "指纹未变时不得重新光栅化");
    let second = second.frame.expect("应复用上一帧");
    assert_eq!(second.version, first.version, "复用帧的版本必须不变");
    assert!(
        Arc::ptr_eq(&first.pixels, &second.pixels),
        "复用帧必须共享同一份像素（不得整帧拷贝）"
    );

    // 文档代次变化 → 重建索引 + 重新光栅化 + 版本递增
    let mut edited_document = document.clone();
    edited_document.insert_note(1, NoteEvent::new(200, 210, 121, 100, 0));
    let mut next_key = key;
    next_key.track_notes_gen = 2;
    let third = state.update(next_key, &edited_document, &muted, None, None);
    assert!(third.rebuilt, "文档代次变化必须重新光栅化");
    let third = third.frame.expect("应生成新帧");
    assert_ne!(third.version, first.version, "新帧版本必须递增");
    assert!(
        !Arc::ptr_eq(&first.pixels, &third.pixels),
        "新帧必须是新的像素缓冲"
    );
}

/// 相机变化（仅滚动）同样要重渲染，且版本递增。
#[test]
fn cpu_frame_rebuilds_on_camera_change() {
    let document = document();
    let muted = vec![false, false, true];
    let base = camera_input(1.0).to_physical().expect("100% 相机应可换算");
    let mut state = CpuOnionState::default();

    let first = state.update(frame_key(base), &document, &muted, None, None);
    let first = first.frame.expect("首帧应生成");

    let mut scrolled_input = camera_input(1.0);
    scrolled_input.scroll = (100.0, 0.0);
    let scrolled = scrolled_input.to_physical().expect("相机应可换算");
    let second = state.update(frame_key(scrolled), &document, &muted, None, None);
    assert!(second.rebuilt, "滚动变化必须重新光栅化");
    assert_ne!(
        second.frame.expect("应生成新帧").version,
        first.version,
        "滚动后版本必须递增"
    );
}

/// 停用后释放缓存：帧消失，再次调用重新光栅化；**版本保持单调递增**。
///
/// 版本不得归零：gfx 按 `(size, version)` 判重，回退会让新会话首帧被误判为
/// `Reuse` 而复用上一会话的旧纹理（`deactivate` 只释放资源、保留版本序列）。
#[test]
fn cpu_frame_deactivate_releases_state() {
    let document = document();
    let muted = vec![false, false, true];
    let geometry = camera_input(1.0).to_physical().expect("100% 相机应可换算");
    let mut state = CpuOnionState::default();
    let key = frame_key(geometry);

    let first = state
        .update(key, &document, &muted, None, None)
        .frame
        .expect("首帧应生成");
    assert_eq!(first.version, 1);
    state.deactivate();
    assert!(state.frame().is_none(), "停用后不得保留帧");

    let rebuilt = state.update(key, &document, &muted, None, None);
    assert!(rebuilt.rebuilt);
    let rebuilt = rebuilt.frame.expect("停用后应可重建");
    assert_eq!(
        rebuilt.version, 2,
        "停用后版本必须继续递增（不得从 1 重新开始）"
    );
    assert_eq!(rebuilt.dirty_rect, None, "重新启用首帧是整帧重绘");
    assert!(
        !Arc::ptr_eq(&first.pixels, &rebuilt.pixels),
        "停用会释放渲染器，重建必须产生新的像素缓冲"
    );
}

/// 静音映射：按文档轨数展开，侧栏越界 id 只进指纹集合、不进静音表。
#[test]
fn muted_mapping_is_bounds_safe() {
    let tracks = [track(0, true), track(1, false), track(5, true)];
    let (muted, ids) = collect_muted_tracks(&tracks, 3);
    assert_eq!(muted, vec![true, false, false], "越界 id 5 不得写入静音表");
    assert_eq!(
        ids,
        vec![0, 5],
        "越界 id 仍要进指纹（切回 GPU 后需要正确静音）"
    );
}

/// 测试用音轨（只关心 id 与静音位）。
fn track(id: usize, is_muted: bool) -> Track {
    Track {
        id,
        name: format!("Track{id}"),
        port: 0,
        channel: 0,
        display_label: format!("A{id:02}"),
        is_conductor: false,
        can_delete: true,
        is_muted,
        is_soloed: false,
        color: None,
    }
}
