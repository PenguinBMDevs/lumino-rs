//! Step 6b — GPU/CPU 洋葱皮像素一致性（可自动化部分）。
//!
//! # 口径
//!
//! 同一份 `MidiDocument`、同一物理相机、同一调色板：
//! 1. CPU：`CpuOnionRenderer::render_full` → 透明背景 RGBA8 sRGB 帧（每侧 2px 边框，
//!    跳过当前轨/静音轨，重叠 first-wins）；
//! 2. GPU：`NoteRenderer::new_onion_skin` + `NoteInstance` + `CameraUniform`
//!    → 离屏 `Rgba8UnormSrgb` + `Depth32Float`（生产主路径
//!    `prepare_pass`(compute cull) + `draw`(indirect)）→ 回读；
//! 3. 逐像素比较：双方 alpha = 0 的背景忽略；其余像素比较覆盖关系、alpha、
//!    RGB（覆盖像素 alpha 必须为 255）。
//!
//! # 颜色为什么能严格比
//!
//! GPU 洋葱皮实例色在 `ui::onion_skin::stream::instances` 里由
//! `current_track_color_f32(track)`（= `调色板字节 / 255.0`）经
//! `pack_key_color` 量化为 8 位；CPU `NoteStyle` 用同一个
//! `current_track_color_f32` 走 `srgb_encode`。两路取色同源，且
//! `pack_key_color` 的 `(byte/255*255)` 8 位往返对 0..=255 全部无损，
//! 因此**只要两边的像素分类一致，RGB 就逐字节相等**（硬件后端实测 0 差）。
//!
//! 唯一的例外是**软件光栅**：CI 的 Linux lavapipe / Windows WARP 在
//! linear→sRGB 写入时的舍入与硬件后端不同，实测最多差 1 级。颜色比较因此
//! 允许 [`diff::COLOR_TOLERANCE`] 级通道差（默认 1），并在报告里单列
//! 「容差内」计数；几何与 alpha 仍然逐像素严格相等，不放宽。
//!
//! # 边界归属
//!
//! 四边等号一律归**填充**，两侧实现同义：
//! - CPU：`lumino_cpu_onion::notes::{fill_start, fill_end}`；
//! - GPU：`onion_note.wgsl::fs_main` 的**屏幕像素空间**判定
//!   （`local = position.xy - rect_min`，`local == 2.0` / `local == size - 2.0` 归填充），
//!   不经过任何重心插值 ⇒ 半像素几何下也没有归属噪声。
//!
//! 因此 100%/125%/150% 三档（含「填充边界恰落像素中心」的退化几何）全部
//! **逐像素严格 0 差异**；另有 125% + 0.3px 半像素滚动用例覆盖非退化小数几何。
//!
//! # 运行
//!
//! ```text
//! cargo test -p lumino-gfx --test cpu_gpu_parity -- --nocapture
//! ```

mod diff;
mod harness;
mod scenes;

use std::sync::Arc;

use diff::{Diffs, compare};
use harness::{NO_MAIN_TRACK, W, camera, instances_for, lock_gpu, render_cpu, render_gpu};
use lumino_cpu_onion::CpuFrame;
use lumino_cpu_onion::TrackIndex;
use lumino_cpu_onion::color::{MAIN_TRACK_LINEAR, NoteStyle, linear_to_argb};
use scenes::Case;

/// CPU + GPU 各渲染一帧并逐像素比较，打印报告后返回两侧结果。
fn run(case: &Case) -> (CpuFrame, Vec<u8>, Diffs) {
    let document = Arc::new(case.document.clone());
    let index = Arc::new(TrackIndex::build(&document));
    let cam = camera(case.zoom, case.scroll, case.key_count);

    let cpu = render_cpu(&document, &index, &cam, case.current_track, &case.muted);
    let instances = instances_for(&document, case.current_track, &case.muted);
    let gpu = render_gpu(&instances, &cam, NO_MAIN_TRACK, &[]);
    let diffs = compare(&cpu, &gpu);

    println!(
        "[{}] 实例 {} 个（排除当前轨 {} / 静音 {:?}）",
        case.label,
        instances.len(),
        case.current_track,
        case.muted
            .iter()
            .enumerate()
            .filter_map(|(track, muted)| muted.then_some(track))
            .collect::<Vec<_>>()
    );
    println!("[{}] {}", case.label, diffs.summary());
    (cpu, gpu, diffs)
}

/// 严格一致断言（几何 + 颜色 + alpha 全部逐像素相同）。
fn assert_exact(label: &str, diffs: &Diffs) {
    assert!(
        diffs.geometry.is_empty(),
        "{label}：覆盖集合不一致 {} 处，首个 {:?}",
        diffs.geometry.len(),
        diffs.geometry.first()
    );
    assert!(
        diffs.alpha_violations.is_empty(),
        "{label}：覆盖像素 alpha 非 255 {} 处，首个 {:?}",
        diffs.alpha_violations.len(),
        diffs.alpha_violations.first()
    );
    assert!(
        diffs.color.is_empty(),
        "{label}：RGB 超出容差 {} 处（最大通道差 {}，容差内 {}），首个 {:?}",
        diffs.color.len(),
        diffs.max_channel_delta,
        diffs.tolerated,
        diffs.color.first()
    );
    assert!(diffs.covered > 0, "{label}：两侧都没有绘制任何音符");
}

/// 用例 1：单轨单音——矩形位置、2px 边框、内部填充。
#[test]
fn parity_single_note_geometry_and_border() {
    let _guard = lock_gpu();
    let case = scenes::single_note();
    let (cpu, _gpu, diffs) = run(&case);
    assert_exact(case.label, &diffs);

    // 硬约束抽检（不依赖 GPU）：填充 [10,46) × [22,28)、边框每侧 2px。
    let style = NoteStyle::new(2);
    let fill = style.fill_rgba(1);
    let border = style.border_rgba(1);
    assert_eq!(cpu.pixel(10, 22), Some(fill), "填充左上像素");
    assert_eq!(cpu.pixel(45, 27), Some(fill), "填充右下像素");
    assert_eq!(cpu.pixel(9, 22), Some(border), "左边框");
    assert_eq!(cpu.pixel(46, 22), Some(border), "右边框");
    assert_eq!(cpu.pixel(10, 21), Some(border), "上边框");
    assert_eq!(cpu.pixel(10, 28), Some(border), "下边框");
    assert_eq!(cpu.pixel(8, 20), Some(border), "矩形左上角");
    assert_eq!(cpu.pixel(7, 20), Some([0, 0, 0, 0]), "矩形左侧为背景");
}

/// 用例 2：同 `start_tick` 长/短音源顺序（Step 6a 主题）的像素证据。
#[test]
fn parity_same_tick_long_short_source_order() {
    let _guard = lock_gpu();
    let case = scenes::same_tick_order();
    let (cpu, _gpu, diffs) = run(&case);
    assert_exact(case.label, &diffs);

    // 自检：两条轨在「短音右边框」区域必须呈现不同像素，否则本用例没有真正
    // 检验顺序（短音先插 → 边框可见；长音先插 → 被填充吃掉）。
    let style = NoteStyle::new(3);
    let short_first_border = cpu.pixel(39, 45).expect("帧内像素");
    let long_first_fill = cpu.pixel(39, 75).expect("帧内像素");
    assert_eq!(
        short_first_border,
        style.border_rgba(1),
        "轨道 1（短音先插）在 x=39 必须显示短音的右边框"
    );
    assert_eq!(
        long_first_fill,
        style.fill_rgba(2),
        "轨道 2（长音先插）在 x=39 必须是长音填充（短音边框被 first-wins 吃掉）"
    );
    assert_ne!(
        short_first_border, long_first_fill,
        "两种源顺序必须产生可区分的像素（否则该用例无鉴别力）"
    );
}

/// 用例 3：多轨不同颜色 / 跨轨重叠 first-wins / 当前轨与静音轨排除。
#[test]
fn parity_multi_track_overlap_and_exclusion() {
    let _guard = lock_gpu();
    let case = scenes::multi_track_overlap();
    let (cpu, _gpu, diffs) = run(&case);
    assert_exact(case.label, &diffs);

    let style = NoteStyle::new(6);
    // 重叠区：轨道索引小者（轨道 0）先画 → 占住像素。
    let overlap = cpu.pixel(50, 65).expect("帧内像素");
    let only_track1 = cpu.pixel(100, 65).expect("帧内像素");
    assert_eq!(overlap, style.fill_rgba(0), "重叠区必须是轨道 0 的填充");
    assert_eq!(
        only_track1,
        style.fill_rgba(1),
        "非重叠区必须是轨道 1 的填充"
    );
    assert_ne!(overlap, only_track1, "两轨颜色必须可区分");
    // 当前轨（轨道 3）与静音轨（轨道 4）必须完全不绘制。
    assert_eq!(
        cpu.pixel(50, 35),
        Some([0, 0, 0, 0]),
        "当前轨不得由 CPU 洋葱皮绘制"
    );
    assert_eq!(cpu.pixel(50, 125), Some([0, 0, 0, 0]), "静音轨不得绘制");
    // 排除按轨道而不是按区域：与当前轨同 key 的另一轨必须照常绘制。
    assert_eq!(
        cpu.pixel(100, 35),
        Some(style.fill_rgba(5)),
        "同 key 的非当前轨必须绘制"
    );
    assert_eq!(
        cpu.pixel(50, 95),
        Some(style.fill_rgba(2)),
        "独占行必须绘制"
    );
}

/// 用例 4：100% 物理相机（基线，必须逐像素严格一致）。
#[test]
fn parity_dpi_100() {
    let _guard = lock_gpu();
    let case = scenes::dpi(1.0);
    let (_cpu, _gpu, diffs) = run(&case);
    assert!(diffs.is_exact(), "DPI 100%：{}", diffs.summary());
    assert_exact("DPI 100%", &diffs);
}

/// 用例 4b：125% 物理相机（`x` 与 `y` 同时出现 `.75/.25/.5` 小数坐标）。
///
/// 该刻度刻意保留「填充边界恰好落在像素中心」的退化点（见 `scenes::dpi` 说明），
/// 归属由着色器的像素空间判定保证，必须逐像素严格一致。
#[test]
fn parity_dpi_125() {
    let _guard = lock_gpu();
    let case = scenes::dpi(1.25);
    let (cpu, _gpu, diffs) = run(&case);
    assert!(diffs.is_exact(), "DPI 125%：{}", diffs.summary());
    assert_exact("DPI 125%", &diffs);

    // 自检：小数几何必须真的生效（不是退化成整数几何）。
    // 轨道 1 首音在 125%、scroll=0 下 sx = 3.75、sw = 8.75：
    // 填充区间 = [ceil(5.75-0.5), floor(10.5-0.5)+1) = [6, 11)；
    // 若为整数几何（100%）则是 [5, 8)。⇒ x=5 在 125% 下必须是边框。
    let style = NoteStyle::new(4);
    assert_eq!(
        cpu.pixel(6, 18),
        Some(style.fill_rgba(1)),
        "x=6 必须是填充（填充左界 5.75 落在像素中心之间）"
    );
    assert_eq!(
        cpu.pixel(5, 18),
        Some(style.border_rgba(1)),
        "x=5 必须是边框：填充左界 5.75 > 像素 5 的中心 5.5（小数取整生效）"
    );
    assert_eq!(
        cpu.pixel(11, 18),
        Some(style.border_rgba(1)),
        "x=11 必须是右边框（填充右界 10.5 = 像素 10 的中心，等号归填充 ⇒ 10 填充、11 边框）"
    );
}

/// 用例 5：125% + 半像素滚动（非退化小数几何），同样要求逐像素严格一致。
#[test]
fn parity_dpi_half_pixel_scroll() {
    let _guard = lock_gpu();
    let case = scenes::dpi_half_pixel_scroll();
    let (_cpu, _gpu, diffs) = run(&case);
    assert!(diffs.is_exact(), "{}：{}", case.label, diffs.summary());
    assert_exact(case.label, &diffs);
}

/// 用例 6：超宽音符（1e6 px）的边框仍是 2px——锁死「宽度相关的边框收敛」回归。
#[test]
fn parity_wide_note_keeps_2px_border() {
    let _guard = lock_gpu();
    let case = scenes::wide_note();
    let (cpu, _gpu, diffs) = run(&case);
    assert!(diffs.is_exact(), "{}：{}", case.label, diffs.summary());
    assert_exact(case.label, &diffs);

    let style = NoteStyle::new(2);
    // key 38 → y ∈ [20, 30)；x 从 0 起铺满视口（音符宽 1e6 px）。
    for x in [0u32, 1] {
        assert_eq!(
            cpu.pixel(x, 25),
            Some(style.border_rgba(1)),
            "超宽音符的左边框第 {x} 列必须仍是描边（不得被宽度相关的收敛吃掉）"
        );
    }
    assert_eq!(
        cpu.pixel(2, 25),
        Some(style.fill_rgba(1)),
        "超宽音符从第 2 列起必须是填充"
    );
    assert_eq!(
        cpu.pixel(255, 25),
        Some(style.fill_rgba(1)),
        "超宽音符视口右端仍是填充（没有右边界落在视口内）"
    );
}

/// 用例 4c：150% 物理相机（`x` 出现 `.5` 小数坐标）。
#[test]
fn parity_dpi_150() {
    let _guard = lock_gpu();
    let case = scenes::dpi(1.5);
    let (_cpu, _gpu, diffs) = run(&case);
    assert!(diffs.is_exact(), "DPI 150%：{}", diffs.summary());
    assert_exact("DPI 150%", &diffs);
}

/// GPU 侧 `ViewState` 语义：全量实例一起上传时，主音轨染主轨蓝、静音轨完全裁剪。
///
/// 这条与 CPU 不等价（CPU 模式下当前轨由 GPU 画、CPU 只画洋葱皮），因此单独断言
/// 「GPU 对同一批实例的排除/主轨着色规则」，补齐 CPU/GPU 分工两侧的证据。
#[test]
fn gpu_view_state_culls_muted_and_paints_current_track() {
    let _guard = lock_gpu();
    let case = scenes::multi_track_overlap();
    let document = Arc::new(case.document.clone());
    let cam = camera(case.zoom, case.scroll, case.key_count);
    // 全量实例（不预先过滤），由 shader 按 ViewState 决定显示方式。
    let all = instances_for(&document, usize::MAX, &[]);
    // 主音轨编码 = 轨道索引 + 1；静音轨道索引 = 4。
    let pixels = render_gpu(&all, &cam, 3 + 1, &[4]);

    let style = NoteStyle::new(6);
    let main_rgba = {
        let argb = linear_to_argb(MAIN_TRACK_LINEAR);
        [
            ((argb >> 16) & 0xFF) as u8,
            ((argb >> 8) & 0xFF) as u8,
            (argb & 0xFF) as u8,
            255,
        ]
    };
    let pixel_at = |x: u32, y: u32| -> [u8; 4] {
        let offset = ((y * W + x) * 4) as usize;
        [
            pixels[offset],
            pixels[offset + 1],
            pixels[offset + 2],
            pixels[offset + 3],
        ]
    };

    let main = pixel_at(50, 35);
    let muted = pixel_at(50, 125);
    let onion = pixel_at(50, 95);
    println!(
        "[GPU ViewState] 主轨 {main:?}（期望 {main_rgba:?}）｜静音轨 {muted:?}（期望全透明）｜洋葱轨 {onion:?}（期望 {:?}）",
        style.fill_rgba(2)
    );
    for channel in 0..3 {
        assert!(
            main[channel].abs_diff(main_rgba[channel]) <= 1,
            "主音轨像素 {main:?} 与主轨常量 {main_rgba:?} 在通道 {channel} 上偏差过大"
        );
    }
    assert_eq!(main[3], 255, "主音轨 alpha 必须为 255");
    assert_eq!(muted, [0, 0, 0, 0], "静音轨必须完全不产生片元");
    assert_eq!(onion, style.fill_rgba(2), "洋葱皮轨道必须用调色板实例色");
}
