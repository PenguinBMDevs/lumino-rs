//! DPI / 大 tick 落点精度契约（与 sort 主测试共用父模块 fixture）。

use super::*;

#[test]
fn test_splat_dpi_scale_matches_point_path() {
    // 逻辑 320×240 → 物理 640×480（scale=2）：落点取整必须与硬件点图元一致
    let camera = CameraUniform {
        viewport_size: [W as f32, H as f32],
        zoom: [1.0, 1.0],
        ..camera()
    };
    let notes = scene();
    let physical = (W * 2, H * 2);
    let point = render_points_with(&notes, camera, physical.0, physical.1);
    let splat = render_splat_with(&notes, camera, physical.0, physical.1);
    assert_eq!(
        point, splat,
        "DPI scale=2 下 splat 与点图元路径必须逐像素一致"
    );
}

#[test]
fn test_splat_large_tick_precision_matches_point_path() {
    // 黑乐谱大 tick：`(tick + length/2)` 的 f32 加会吃掉半长，
    // 必须先算 tick 的屏幕位置、再加半长（与 vs_point 同序）。
    let camera = CameraUniform {
        zoom: [W as f32 / 2_000_000_000.0, 1.0],
        ..camera()
    };
    // 半长 0.00032px：大于 f32@160 的 ulp（≈1.5e-5），落点远离像素边界
    let notes = vec![
        NoteInstance::new(1_000_000_000.0, 64, 4000.0, RED, encode(1)),
        NoteInstance::new(1_500_000_000.0, 70, 8000.0, GREEN, encode(1)),
    ];
    let point = render_points_with(&notes, camera, W, H);
    let splat = render_splat_with(&notes, camera, W, H);
    let mut diffs = 0usize;
    for y in 0..H {
        for x in 0..W {
            let p = pixel(&point, x, y);
            let s = pixel(&splat, x, y);
            if p != s {
                diffs += 1;
                if diffs <= 12 {
                    eprintln!("diff ({x},{y}) point={p:?} splat={s:?}");
                }
            }
        }
    }
    assert_eq!(
        diffs, 0,
        "大 tick 下 splat 与点图元路径必须逐像素一致（差异 {diffs} 像素）"
    );
}
