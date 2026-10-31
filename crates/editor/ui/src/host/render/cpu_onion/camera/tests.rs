//! 相机换算验收：**网格内容区矩形**（去掉键盘栏/标尺）、DPI floor 口径、
//! 退化输入跳过，以及**网格对齐语义**（CPU 帧全局坐标 ≈ GPU 网格内容坐标）。
//!
//! 与 `camera.rs` 分离是为守住「单文件 < 400 行」硬门槛（本仓约定同 `plan/tests.rs`）。

use super::*;

/// 基准键盘栏宽 / 标尺高（逻辑像素，量级同工程默认 120 / 24）。
const KEYBOARD_WIDTH: f32 = 60.0;
const RULER_HEIGHT: f32 = 30.0;

/// 基准输入：画布偏移 (60, 30)、尺寸 (1000, 500)、滚动 (100, 50)、缩放 (2, 20)。
fn input(scale_factor: f32) -> CpuOnionCameraInput {
    CpuOnionCameraInput {
        scale_factor,
        canvas_offset: (60.0, 30.0),
        canvas_size: (1000.0, 500.0),
        keyboard_width: KEYBOARD_WIDTH,
        ruler_height: RULER_HEIGHT,
        scroll: (100.0, 50.0),
        zoom: (2.0, 20.0),
        key_count: 128,
    }
}

/// 100%：内容区 = canvas 去掉 (60, 30) 的内缩，相机仍是内容区局部坐标。
#[test]
fn physical_geometry_at_100_percent() {
    let geometry = input(1.0).to_physical().expect("100% 应可换算");
    // 内容区原点 = canvas 偏移 + 键盘栏/标尺；尺寸 = canvas 尺寸 − 内缩
    assert_eq!(
        (geometry.x, geometry.y),
        (120, 60),
        "内容区起点必须右移/下移内缩量"
    );
    assert_eq!(
        (geometry.width, geometry.height),
        (940, 470),
        "内容区尺寸必须减去键盘栏/标尺"
    );
    // 回归防呆：修复前这里拿到的是**整块 canvas**（60/30、1000/500），
    // CPU 音符因此相对 GPU 网格整体左移 keyboard_width、上移 ruler_height。
    assert_ne!(
        (geometry.x, geometry.y),
        (60, 30),
        "不得回退到整块 canvas 起点（原 bug 就是把它当内容区）"
    );
    assert_ne!(
        (geometry.width, geometry.height),
        (1000, 500),
        "不得回退到整块 canvas 尺寸（会把键盘栏/标尺也光栅进帧）"
    );
    assert_eq!(geometry.camera.width, 940);
    assert_eq!(geometry.camera.height, 470);
    // 相机参数（滚动/缩放/键数）不受内缩影响
    assert_eq!(geometry.camera.scroll_x, 100.0);
    assert_eq!(geometry.camera.scroll_y, 50.0);
    assert_eq!(geometry.camera.zoom_x, 2.0);
    assert_eq!(geometry.camera.zoom_y, 20.0);
    assert_eq!(geometry.camera.key_count, 128);
}

/// 125%：内容区四边全部 ×1.25 后 floor（470 × 1.25 = 587.5 → 587，半像素向下取整）。
#[test]
fn physical_geometry_at_125_percent() {
    let geometry = input(1.25).to_physical().expect("125% 应可换算");
    assert_eq!((geometry.x, geometry.y), (150, 75), "120×1.25 / 60×1.25");
    assert_eq!(
        (geometry.width, geometry.height),
        (1175, 587),
        "940×1.25=1175；470×1.25=587.5 必须向下取整"
    );
    assert_eq!(geometry.camera.width, 1175);
    assert_eq!(geometry.camera.height, 587);
    assert_eq!(geometry.camera.scroll_x, 125.0);
    assert_eq!(geometry.camera.scroll_y, 62.5);
    assert_eq!(geometry.camera.zoom_x, 2.5);
    assert_eq!(geometry.camera.zoom_y, 25.0);
}

/// 150%：内容区四边 ×1.5（不含半像素，但必须与相机同口径）。
#[test]
fn physical_geometry_at_150_percent() {
    let geometry = input(1.5).to_physical().expect("150% 应可换算");
    assert_eq!((geometry.x, geometry.y), (180, 90));
    assert_eq!((geometry.width, geometry.height), (1410, 705));
    assert_eq!(geometry.camera.width, 1410);
    assert_eq!(geometry.camera.height, 705);
    assert_eq!(geometry.camera.scroll_x, 150.0);
    assert_eq!(geometry.camera.scroll_y, 75.0);
    assert_eq!(geometry.camera.zoom_x, 3.0);
    assert_eq!(geometry.camera.zoom_y, 30.0);
}

/// 小数逻辑值先做内缩再 floor（与渲染通道 canvas scissor 的口径一致）。
#[test]
fn fractional_logical_values_floor() {
    let mut input = input(1.25);
    input.canvas_offset = (60.7, 30.2);
    input.canvas_size = (1000.9, 500.1);
    let geometry = input.to_physical().expect("小数逻辑值应可换算");
    // x：(60.7 + 60) × 1.25 = 150.875 → 150
    // y：(30.2 + 30) × 1.25 = 75.25   → 75
    assert_eq!((geometry.x, geometry.y), (150, 75));
    // w：(1000.9 − 60) × 1.25 = 1176.125 → 1176
    // h：(500.1 − 30) × 1.25 = 587.625  → 587
    assert_eq!((geometry.width, geometry.height), (1176, 587));
}

/// 键盘栏/标尺为 0 时与原口径完全一致（防回归：无内缩的布局不受影响）。
#[test]
fn zero_margins_match_whole_canvas() {
    let mut input = input(1.0);
    input.keyboard_width = 0.0;
    input.ruler_height = 0.0;
    let geometry = input.to_physical().expect("无边栏应可换算");
    assert_eq!((geometry.x, geometry.y), (60, 30));
    assert_eq!((geometry.width, geometry.height), (1000, 500));

    input.scale_factor = 1.25;
    let geometry = input.to_physical().expect("无边栏 125% 应可换算");
    assert_eq!((geometry.x, geometry.y), (75, 37), "offset_y 37.5 向下取整");
    assert_eq!((geometry.width, geometry.height), (1250, 625));
}

/// 键盘栏/标尺把画布吃光（或超出画布）→ 不画（`None`），绝不产生负尺寸帧。
#[test]
fn margins_larger_than_canvas_are_skipped() {
    // 键盘栏比画布宽
    let mut wide_keyboard = input(1.0);
    wide_keyboard.keyboard_width = 1200.0;
    assert!(wide_keyboard.to_physical().is_none());
    // 标尺比画布高
    let mut tall_ruler = input(1.0);
    tall_ruler.ruler_height = 600.0;
    assert!(tall_ruler.to_physical().is_none());
    // 正好相等 → 内容区尺寸为 0，同样不画
    let mut exact = input(1.0);
    exact.keyboard_width = 1000.0;
    assert!(exact.to_physical().is_none());
    let mut exact_height = input(1.0);
    exact_height.ruler_height = 500.0;
    assert!(exact_height.to_physical().is_none());
    // 高 DPI 下内缩后不足 1 物理像素 → 也跳过（0 尺寸帧无意义）
    let mut hairline = input(1.5);
    hairline.canvas_size = (1000.0, 30.4);
    assert!(
        hairline.to_physical().is_none(),
        "30.4−30=0.4 逻辑 → 0 物理"
    );
}

/// 非法边栏（非有限 / 负数）→ `None`（布局异常不猜坐标）。
#[test]
fn invalid_margins_are_skipped() {
    for bad in [f32::NAN, f32::INFINITY, -1.0] {
        let mut keyboard = input(1.0);
        keyboard.keyboard_width = bad;
        assert!(
            keyboard.to_physical().is_none(),
            "keyboard_width={bad} 必须跳过"
        );
        let mut ruler = input(1.0);
        ruler.ruler_height = bad;
        assert!(ruler.to_physical().is_none(), "ruler_height={bad} 必须跳过");
    }
}

/// 其余退化输入一律跳过（不生成错位帧）。
#[test]
fn degenerate_inputs_are_skipped() {
    // 缩放因子非法
    for scale in [0.0f32, -1.0, f32::NAN, f32::INFINITY] {
        assert!(
            input(scale).to_physical().is_none(),
            "scale={scale} 必须跳过"
        );
    }
    // 画布宽/高为 0（未布局 / 面板隐藏）
    let mut zero_size = input(1.0);
    zero_size.canvas_size = (0.0, 500.0);
    assert!(zero_size.to_physical().is_none());
    zero_size.canvas_size = (1000.0, 0.0);
    assert!(zero_size.to_physical().is_none());
    // 内容区原点为负（画布偏移为负且超过边栏内缩）→ 布局异常，跳过
    let mut negative = input(1.0);
    negative.canvas_offset = (-100.0, 0.0);
    assert!(negative.to_physical().is_none());
    // 负偏移被键盘栏内缩吸收时原点是正的：这是**精确**换算（GPU 同样按
    // `offset + keyboard_width` 定位内容区），照常绘制而不是静默丢帧
    let mut absorbed = input(1.0);
    absorbed.canvas_offset = (-1.0, 0.0);
    let absorbed = absorbed.to_physical().expect("内容区原点为正应可换算");
    assert_eq!(
        (absorbed.x, absorbed.y),
        (59, 30),
        "x = −1 + 60 = 59；y = 0 + 30 = 30"
    );
    // 滚动/缩放非有限
    let mut nan_zoom = input(1.0);
    nan_zoom.zoom = (f32::NAN, 20.0);
    assert!(nan_zoom.to_physical().is_none());
    let mut inf_scroll = input(1.0);
    inf_scroll.scroll = (f32::INFINITY, 0.0);
    assert!(inf_scroll.to_physical().is_none());
}

/// GPU 网格口径下的内容区**物理** X（`infinite_grid.wgsl` 横向公式的逆运算）。
///
/// 片元里的 `screen_x` 是逻辑像素（uniform 传的是 `logical_size` 与逻辑
/// `canvas_offset` / `margins` / `scroll` / `zoom`），所以物理屏幕坐标 =
/// `(canvas_offset.x + keyboard_width + tick·zoom_x − scroll_x) × scale`。
fn gpu_content_physical_x(input: &CpuOnionCameraInput, tick: f32) -> f32 {
    (input.canvas_offset.0 + input.keyboard_width + tick * input.zoom.0 - input.scroll.0)
        * input.scale_factor
}

/// GPU 网格口径下的内容区**物理** Y（纵向公式的逆运算）。
///
/// `local_y = screen_y − ruler_height − canvas_offset.y + scroll_y`，
/// `key = max_key − local_y / zoom_y` ⇒ `screen_y = canvas_offset.y + ruler_height
/// + (max_key − key)·zoom_y − scroll_y`。
fn gpu_content_physical_y(input: &CpuOnionCameraInput, key: u16) -> f32 {
    let max_key = input.key_count.saturating_sub(1) as f32;
    (input.canvas_offset.1 + input.ruler_height + (max_key - key as f32) * input.zoom.1
        - input.scroll.1)
        * input.scale_factor
}

/// 断言该 tick/key 的 CPU 全局物理坐标与 GPU 网格坐标对齐。
///
/// 唯一允许的偏差是帧原点 `floor(逻辑值 × scale)` 的取整预算：∈ [−1, 0] px。
/// 修复前的整块 canvas 起点会带来 `keyboard_width × scale`（本例 60~90px）与
/// `ruler_height × scale` 的系统性偏移，远出容差——本断言能直接抓住原 bug。
fn assert_aligned(input: CpuOnionCameraInput, tick: f32, key: u16) {
    let geometry = input.to_physical().expect("常规布局应可换算");
    let cpu_x = geometry.x as f32 + geometry.camera.tick_to_x(tick);
    let cpu_y = geometry.y as f32 + geometry.camera.key_to_y(key);
    for (axis, cpu, gpu) in [
        ("x", cpu_x, gpu_content_physical_x(&input, tick)),
        ("y", cpu_y, gpu_content_physical_y(&input, key)),
    ] {
        let diff = cpu - gpu;
        assert!(
            (-1.0 - 1e-3..=1e-3).contains(&diff),
            "scale={} tick={tick} key={key} 轴 {axis}：CPU 物理 {cpu} / GPU 网格 {gpu} \
             差 {diff} px（应 ∈ [−1, 0]）",
            input.scale_factor
        );
    }
}

/// **核心语义**：CPU 洋葱皮音符必须画在 GPU 网格线上（100% / 125% / 150%）。
#[test]
fn cpu_frame_aligns_with_gpu_grid_at_all_dpi() {
    for scale in [1.0f32, 1.25, 1.5] {
        // (tick, key) 都落在内容区可见范围内（内容区 940×470 逻辑，缩放 2 / 20）
        for (tick, key) in [(60.0f32, 127u16), (200.0, 120), (400.0, 110)] {
            assert_aligned(input(scale), tick, key);
        }
    }
}

/// 缩放（X/Y）后，CPU 帧的 key 行仍必须与左侧键盘/网格同一公式。
///
/// 这条用例锁的是「缩放只改相机，不改内容区原点」：若帧原点被误当成随
/// zoom/scroll 变化，或帧高被多减一次 ruler，key 行会整体上/下移。
#[test]
fn cpu_key_rows_align_with_keyboard_after_zoom() {
    for (zoom_x, zoom_y, scroll_x, scroll_y) in [
        (2.0f32, 20.0f32, 100.0f32, 50.0f32),
        (0.5, 6.0, 0.0, 0.0),
        (8.0, 40.0, 1234.5, 987.25),
    ] {
        let mut input = input(1.0);
        input.zoom = (zoom_x, zoom_y);
        input.scroll = (scroll_x, scroll_y);
        let geometry = input.to_physical().expect("常规缩放应可换算");
        let max_key = (input.key_count - 1) as f32;

        // 帧矩形不随缩放漂移：仍等于 canvas 去掉键盘栏/标尺后的内容区。
        assert_eq!(
            (geometry.x, geometry.y),
            (120, 60),
            "缩放不得改变内容区原点；zoom=({zoom_x},{zoom_y})"
        );
        assert_eq!(
            (geometry.width, geometry.height),
            (940, 470),
            "缩放不得改变内容区尺寸；zoom=({zoom_x},{zoom_y})"
        );
        assert_eq!(
            geometry.y + geometry.height,
            (input.canvas_offset.1 + input.canvas_size.1) as u32,
            "帧底边必须贴住 canvas 底边（不能少减/多减 ruler）"
        );

        for key in [0u16, 60, 120, 127] {
            let cpu_y = geometry.y as f32 + geometry.camera.key_to_y(key);
            let keyboard_y =
                input.canvas_offset.1 + input.ruler_height + (max_key - key as f32) * zoom_y
                    - scroll_y;
            let diff = cpu_y - keyboard_y;
            assert!(
                diff.abs() <= 1.0,
                "zoom=({zoom_x},{zoom_y}) key={key}：CPU 帧 {cpu_y} / 键盘 {keyboard_y} 差 {diff}px"
            );
        }
    }
}

/// 内容区原点为**半像素**（floor 真正起作用）时仍必须对齐。
#[test]
fn cpu_frame_aligns_with_fractional_content_origin() {
    let mut input = input(1.25);
    // (60 + 45) × 1.25 = 131.25 → floor 131；半像素偏差只能来自这里的取整
    input.keyboard_width = 45.0;
    // (30 + 33) × 1.25 = 78.75 → floor 78
    input.ruler_height = 33.0;
    assert_eq!(input.to_physical().expect("应可换算").x, 131);
    for (tick, key) in [(60.0f32, 127u16), (200.0, 120), (400.0, 110)] {
        assert_aligned(input, tick, key);
    }

    input.scale_factor = 1.5;
    for (tick, key) in [(60.0f32, 127u16), (200.0, 120), (400.0, 110)] {
        assert_aligned(input, tick, key);
    }
}

/// 横向滚动吸附：100% / 125% DPI 都必须落在整数物理像素，退化输入原样返回。
#[test]
fn snap_scroll_x_lands_on_physical_pixels() {
    for (logical, scale) in [
        (10.4f32, 1.0f32),
        (10.6, 1.0),
        (10.4, 1.25),
        (10.24, 1.25),
        (10.8, 1.25),
        (7.3, 1.5),
    ] {
        let snapped = super::snap_scroll_x(logical, scale);
        let physical = snapped * scale;
        assert!(
            (physical - physical.round()).abs() < 1e-4,
            "logical={logical} scale={scale}：吸附后物理值 {physical} 必须是整数"
        );
    }
    let nan = super::snap_scroll_x(f32::NAN, 1.0);
    assert!(nan.is_nan(), "NaN 必须原样返回，不得猜");
    assert_eq!(super::snap_scroll_x(10.0, 0.0), 10.0);
    assert_eq!(super::snap_scroll_x(10.0, -1.0), 10.0);
}
