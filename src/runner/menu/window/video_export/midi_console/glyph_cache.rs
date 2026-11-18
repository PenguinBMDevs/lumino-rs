//! MidiConsole CPU 后端：字形栅格缓存。
//!
//! `ab_glyph` 的 `outline_glyph` + `draw` 每次调用都会重新展平曲线并扫描线
//! 栅格化；1080p 单帧 ~2000 个字符单元实测占 17ms。覆盖率只与「字符 + 基线
//! 亚像素相位」有关，与颜色、行号无关，因此按 `(char, x_fract, y_fract)` 缓存
//! 覆盖率样本，绘制时只做颜色混合。
//!
//! **像素等价**：缓存保存的是原 `draw` 回调给出的 f32 覆盖率与顺序，包围盒
//! 左上角按 `base_min + position.trunc()` 重建（`px_bounds` 的 floor 项只依赖
//! 亚像素相位，故整数值可精确复用），输出与逐帧重新栅格化逐位一致。

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use ab_glyph::{Font, FontArc, PxScale, point};

/// 单个字符在给定亚像素相位下的栅格结果。
pub(super) struct CachedGlyph {
    /// 像素包围盒左上角相对 `position.trunc()` 的偏移（整数精度 f32）
    pub(super) base_min: (f32, f32),
    /// `(包围盒局部 x, y, 覆盖率)`，顺序与 `OutlinedGlyph::draw` 一致。
    ///
    /// 坐标预转 `i32`：`px_bounds.min` 恒为整数值，原实现的
    /// `(px as f32 + b.min).round() as i32` 等价于 `b.min as i32 + px`，
    /// 逐样本省去浮点加/取整/转换。
    pub(super) samples: Vec<(i32, i32, f32)>,
}

#[derive(Default)]
struct GlyphCache {
    /// `PxScale` 位模式：分辨率/字号变化即整体失效，避免复用旧尺寸的覆盖率
    scale: (u32, u32),
    map: HashMap<(char, u32, u32), Rc<CachedGlyph>>,
}

thread_local! {
    static CACHE: RefCell<GlyphCache> = RefCell::new(GlyphCache::default());
}

/// 缓存条目上限：超过即整体清空（避免异常分辨率下 148 列 × 40 行相位组合撑爆内存）
const MAX_ENTRIES: usize = 4096;

/// 取缓存字形；未命中时现场栅格化并缓存。
pub(super) fn cached_glyph(
    font: &FontArc,
    scale: PxScale,
    ch: char,
    baseline_x: f32,
    baseline_y: f32,
) -> Rc<CachedGlyph> {
    let x_frac = baseline_x - baseline_x.trunc();
    let y_frac = baseline_y - baseline_y.trunc();
    let key = (ch, x_frac.to_bits(), y_frac.to_bits());
    let scale_bits = (scale.x.to_bits(), scale.y.to_bits());
    CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        if cache.scale != scale_bits {
            cache.scale = scale_bits;
            cache.map.clear();
        }
        if let Some(glyph) = cache.map.get(&key) {
            return Rc::clone(glyph);
        }
        if cache.map.len() >= MAX_ENTRIES {
            cache.map.clear();
        }
        let glyph = Rc::new(rasterize(font, scale, ch, baseline_x, baseline_y));
        cache.map.insert(key, Rc::clone(&glyph));
        glyph
    })
}

/// 原样复刻 `render_midicomsole_frame` 的栅格化参数。
fn rasterize(font: &FontArc, scale: PxScale, ch: char, bx: f32, by: f32) -> CachedGlyph {
    let mut samples = Vec::new();
    let mut base_min = (0.0f32, 0.0f32);
    let gid = font.glyph_id(ch);
    if let Some(outline) = font.outline_glyph(gid.with_scale_and_position(scale, point(bx, by))) {
        let b = outline.px_bounds();
        base_min = (b.min.x - bx.trunc(), b.min.y - by.trunc());
        outline.draw(|px, py, cov| samples.push((px as i32, py as i32, cov)));
    }
    CachedGlyph { base_min, samples }
}
