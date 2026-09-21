//! MidiConsole GPU 渲染器 — 等宽字体加载与字形图集烘焙

use std::path::PathBuf;

use ab_glyph::{Font, FontArc, FontVec, Point, PxScale, ScaleFont};

use super::{ATLAS_CELL_H, ATLAS_CELL_W, ATLAS_COLS, ATLAS_ROWS};

/// 加载等宽字体（与 CPU 预览一致：Consolas / DejaVu 候选）
fn load_monospace_font() -> Option<FontArc> {
    let candidates: Vec<PathBuf> = if cfg!(windows) {
        let dir =
            std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string()) + "\\Fonts\\";
        vec![
            PathBuf::from(dir.clone() + "consola.ttf"),
            PathBuf::from(dir.clone() + "consolab.ttf"),
            PathBuf::from(dir.clone() + "couri.ttf"),
            PathBuf::from(dir + "arial.ttf"),
        ]
    } else {
        vec![
            PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"),
            PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"),
            PathBuf::from("/System/Library/Fonts/Supplemental/Courier New.ttf"),
            PathBuf::from("/Library/Fonts/Courier New.ttf"),
        ]
    };
    for p in &candidates {
        if let Ok(bytes) = std::fs::read(p)
            && let Ok(f) = FontVec::try_from_vec(bytes)
        {
            return Some(FontArc::from(f));
        }
    }
    None
}

/// 烘焙字形图集：ASCII 32..=126 + 半块 ▌(U+258C)，单槽 `ATLAS_CELL_W×ATLAS_CELL_H`，
/// 返回 (r8 覆盖率数据, 宽, 高)。无字体时返回空图集。
pub(super) fn build_glyph_atlas() -> Option<(Vec<u8>, u32, u32)> {
    let font = load_monospace_font()?;
    let atlas_w = ATLAS_COLS * ATLAS_CELL_W;
    let atlas_h = ATLAS_ROWS * ATLAS_CELL_H;
    let mut data = vec![0u8; (atlas_w * atlas_h) as usize];

    let scale = PxScale::from(ATLAS_CELL_H as f32);
    let scaled = font.as_scaled(scale);
    let ascent = scaled.ascent();

    for slot in 0u32..(ATLAS_COLS * ATLAS_ROWS) {
        let ch: u32 = if slot < 95 {
            slot + 32
        } else if slot == 96 {
            0x258C
        } else {
            0
        };
        if ch == 0 {
            continue;
        }
        let gid = font.glyph_id(char::from_u32(ch)?);
        let glyph = gid.with_scale_and_position(scale, Point { x: 0.0, y: ascent });
        let Some(outline) = font.outline_glyph(glyph) else {
            continue;
        };
        let b = outline.px_bounds();
        let slot_x = (slot % ATLAS_COLS) * ATLAS_CELL_W;
        let slot_y = (slot / ATLAS_COLS) * ATLAS_CELL_H;
        outline.draw(|px, py, cov| {
            let x = (px as f32 + b.min.x).round() as i32 + slot_x as i32;
            let y = (py as f32 + b.min.y).round() as i32 + slot_y as i32;
            if x >= 0 && y >= 0 && x < atlas_w as i32 && y < atlas_h as i32 {
                let idx = (y as u32 * atlas_w + x as u32) as usize;
                data[idx] = (cov * 255.0).clamp(0.0, 255.0) as u8;
            }
        });
    }

    Some((data, atlas_w, atlas_h))
}
