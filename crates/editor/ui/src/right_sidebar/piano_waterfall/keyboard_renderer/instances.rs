use super::super::key_layout::KeyRect;

use super::{Instance, KeyboardColors};

/// 像素矩形 → clip 空间实例数据（白键 → 黑键排序），并整体下移 `y_offset` 贴底
///
/// 注意：`full_height` 为**整张纹理高度**（用于 clip 空间归一化），与键条局部高度不同。
pub(super) fn build_instances(
    width: u32,
    full_height: f32,
    y_offset: f32,
    keys: &[KeyRect],
    colors: &KeyboardColors,
) -> Vec<Instance> {
    let w = width as f32;
    let h = full_height;
    let mut out = Vec::with_capacity(keys.len());
    for key in keys {
        let color = if key.is_black {
            colors.black
        } else {
            colors.white
        };
        // 像素矩形 → clip 空间（y 向下 → 需翻转），整体下移 y_offset 贴底
        let rx = (key.x / w) * 2.0 - 1.0;
        let ry = 1.0 - (((key.y + y_offset) + key.h) / h) * 2.0;
        let rw = (key.w / w) * 2.0;
        let rh = (key.h / h) * 2.0;
        out.push(Instance {
            rect: [rx, ry, rw, rh],
            color: [color[0], color[1], color[2], color[3]],
            key: key.key,
        });
    }
    out
}

/// 构建音符 uniform 字节（32 字节，std140 布局，见 `NoteUniforms`）
pub(super) fn build_uniforms(
    width: f32,
    height: f32,
    zoom_x: f32,
    scroll_x: f32,
    current_track: u32,
    key_count: u32,
    keyboard_y: f32,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(32);
    for f in [width, height] {
        out.extend_from_slice(&f.to_le_bytes());
    }
    for f in [zoom_x, scroll_x] {
        out.extend_from_slice(&f.to_le_bytes());
    }
    for u in [current_track, key_count] {
        out.extend_from_slice(&u.to_le_bytes());
    }
    for f in [keyboard_y, 0.0f32] {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

/// `&[f32]` → `&[u8]`（小端，避免引入 bytemuck 依赖）
pub(super) fn f32_slice_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

/// `&[Instance]` → `Vec<u8>`（按内存布局逐字段小端写入）
pub(super) fn instances_to_bytes(instances: &[Instance]) -> Vec<u8> {
    let mut out = Vec::with_capacity(instances.len() * 36);
    for inst in instances {
        for f in inst.rect.iter() {
            out.extend_from_slice(&f.to_le_bytes());
        }
        for f in inst.color.iter() {
            out.extend_from_slice(&f.to_le_bytes());
        }
        out.extend_from_slice(&inst.key.to_le_bytes());
    }
    out
}
