//! 数据曲线模式像素绘制原语
//!
//! 全部直写 BGRA 帧数据（in-place），替代原版 love.graphics 逐线调用：
//! - `fill_bgra`：整帧填充
//! - `blend_px`：单像素 alpha 混合
//! - `blend_hline`：带厚度水平线（网格刻度线）
//! - `draw_thick_line`：DDA 步进 + box stamping 粗线段（折线）

use super::data_curve_math::rgba_to_bgra;

/// 用颜色填满整帧（alpha=255 时直接覆盖，否则混合）。
pub(super) fn fill_bgra(frame: &mut [u8], color_rgba: [u8; 4]) {
    let c = rgba_to_bgra(color_rgba);
    if c[3] == 255 {
        for px in frame.as_chunks_mut::<4>().0 {
            px.copy_from_slice(&c);
        }
    } else {
        for px in frame.as_chunks_mut::<4>().0 {
            blend_px(px, c);
        }
    }
}

/// 将颜色 alpha 混合到单个 BGRA 像素上（保留背景 alpha 通道不变）。
pub(super) fn blend_px(dst: &mut [u8], color_bgra: [u8; 4]) {
    let a = color_bgra[3] as u32;
    if a == 255 {
        dst.copy_from_slice(&color_bgra);
        return;
    }
    let inv = 255 - a;
    for i in 0..3 {
        dst[i] = ((color_bgra[i] as u32 * a + dst[i] as u32 * inv) / 255) as u8;
    }
    // alpha 通道保持背景值（帧始终不透明）
}

/// 绘制一条带厚度的水平线（alpha 混合，越界裁剪）。
pub(super) fn blend_hline(
    frame: &mut [u8],
    fw: usize,
    fh: usize,
    y: i32,
    color_bgra: [u8; 4],
    thickness: u32,
) {
    let t = thickness.max(1) as i32;
    for dy in 0..t {
        let row = y + dy - t / 2;
        if row < 0 || row >= fh as i32 {
            continue;
        }
        let start = row as usize * fw * 4;
        let end = start + fw * 4;
        if color_bgra[3] == 255 {
            frame[start..end]
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .for_each(|px| {
                    px.copy_from_slice(&color_bgra);
                });
        } else {
            frame[start..end]
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .for_each(|px| blend_px(px, color_bgra));
        }
    }
}

/// 绘制一条粗线段（DDA 步进 + box stamping，越界裁剪）。
///
/// 整数端点输入；每步在端点处画 `thickness × thickness` 实心方块。
pub(super) struct DrawThickLineInput<'a> {
    pub frame: &'a mut [u8],
    pub fw: usize,
    pub fh: usize,
    pub x0: i64,
    pub y0: i64,
    pub x1: i64,
    pub y1: i64,
    pub color_bgra: [u8; 4],
    pub thickness: u32,
}

pub(super) fn draw_thick_line(input: DrawThickLineInput<'_>) {
    let DrawThickLineInput {
        frame,
        fw,
        fh,
        x0,
        y0,
        x1,
        y1,
        color_bgra,
        thickness,
    } = input;
    let t = thickness.max(1) as i64;
    let half = t / 2;
    let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
    let (dx, dy) = (
        (x1 - x0) as f64 / steps as f64,
        (y1 - y0) as f64 / steps as f64,
    );
    let (mut fx, mut fy) = (x0 as f64, y0 as f64);
    let (fw_i, fh_i) = (fw as i64, fh as i64);
    for _ in 0..=steps {
        let cx = fx.round() as i64;
        let cy = fy.round() as i64;
        for oy in -half..(t - half) {
            let py = cy + oy;
            if py < 0 || py >= fh_i {
                continue;
            }
            let row = py as usize * fw * 4;
            for ox in -half..(t - half) {
                let px = cx + ox;
                if px < 0 || px >= fw_i {
                    continue;
                }
                let idx = row + px as usize * 4;
                if color_bgra[3] == 255 {
                    frame[idx..idx + 4].copy_from_slice(&color_bgra);
                } else {
                    blend_px(&mut frame[idx..idx + 4], color_bgra);
                }
            }
        }
        fx += dx;
        fy += dy;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 线段越界裁剪：起点在画面外也能画出可见部分
    #[test]
    fn test_draw_thick_line_clip() {
        let mut frame = vec![0u8; 64 * 64 * 4];
        let c = [255, 255, 0, 255]; // BGRA 黄色
        draw_thick_line(DrawThickLineInput {
            frame: &mut frame,
            fw: 64,
            fh: 64,
            x0: -5,
            y0: 30,
            x1: 70,
            y1: 30,
            color_bgra: c,
            thickness: 3,
        });
        assert!(frame.as_chunks::<4>().0.contains(&c), "线段应出现在画面内");
    }

    /// 水平线厚度居中：thickness=3 时上下各扩展 1px
    #[test]
    fn test_blend_hline_thickness() {
        let mut frame = vec![0u8; 32 * 16 * 4];
        let c = [255, 255, 255, 255];
        blend_hline(&mut frame, 32, 16, 8, c, 3);
        for y in [7usize, 8, 9] {
            assert_eq!(
                &frame[(y * 32) * 4..(y * 32) * 4 + 4],
                &c,
                "y={y} 应有线像素"
            );
        }
        assert_eq!(
            &frame[(6 * 32) * 4..(6 * 32) * 4 + 4],
            &[0, 0, 0, 0],
            "y=6 无线"
        );
    }

    /// 半透明颜色混合：不透明背景上按 alpha 加权
    #[test]
    fn test_blend_px_alpha() {
        let mut dst = [255u8, 0, 0, 255]; // 背景：纯蓝（BGRA）
        // 前景：纯红 50% alpha（BGRA [0,0,255,127]）
        blend_px(&mut dst, [0, 0, 255, 127]);
        // (0*127 + 255*128)/255 ≈ 128；R 通道 (255*127+0*128)/255 ≈ 127
        assert!(dst[0] > 0, "背景蓝分量应保留");
        assert!(dst[2] > 0, "前景红分量应混入");
        assert_eq!(dst[3], 255, "alpha 保持不透明");
    }
}
