//! Copy along path 的音符落点

use lumino_core::shape::{ColourMode, CopyMode, CopyPathParams, FlipMode, FlipWay, ShapeRect};

use crate::ShapeNote;

use super::path::{Dir, Placement, corner_a, place_on_run, runs};

/// 一次副本落到音符上
pub fn copy_notes(
    notes: &[ShapeNote],
    place: &Placement,
    params: &CopyPathParams,
) -> Vec<ShapeNote> {
    if notes.is_empty() {
        return Vec::new();
    }
    let rect = bbox(notes);
    let width = (rect.tick_hi - rect.tick_lo).max(1e-6);
    let height = (rect.key_hi - rect.key_lo).max(1e-6);
    let flip_time = flip_axis(params.flip, place.dir, FlipWay::Time);
    let flip_keys = flip_axis(params.flip, place.dir, FlipWay::Keys);
    let sx = place.scale[0];
    let sy = place.scale[1];
    notes
        .iter()
        .map(|note| {
            let mut local_x = note.tick - rect.tick_lo;
            if flip_time {
                local_x = width - local_x - note.length;
            }
            let mut local_y = note.key as f32 - rect.key_lo;
            if flip_keys {
                local_y = height - local_y;
            }
            let x = place.corner.0 + local_x * sx;
            let y = match place.dir {
                Dir::Up => place.corner.1 + local_y * sy,
                Dir::Down => place.corner.1 - local_y * sy,
            };
            ShapeNote {
                tick: x.max(0.0),
                key: y.round().clamp(0.0, 255.0) as u16,
                length: (note.length * sx).max(0.0),
                velocity: note.velocity,
                channel: recolour(note.channel, place.index, params),
                ..*note
            }
        })
        .collect()
}

/// 应用整个 CopyAlong 页；返回原音符 + 副本
pub fn apply(notes: &[ShapeNote], params: &CopyPathParams, ppq: f64) -> Vec<ShapeNote> {
    if notes.is_empty() || params.pts.is_empty() {
        return notes.to_vec();
    }
    let rect = bbox(notes);
    let base = corner_a(&rect, Dir::Up);
    let mut out = notes.to_vec();
    for run in runs(params, base) {
        let count = count_for(&run, params, ppq);
        for place in place_on_run(&run, count, params.speed, params.size) {
            out.extend(copy_notes(notes, &place, params));
        }
    }
    out
}

fn count_for(run: &super::path::Run, params: &CopyPathParams, ppq: f64) -> u32 {
    match params.mode {
        CopyMode::Count { n } => n,
        CopyMode::Every { beats } => {
            let spacing = f64::from(beats) * ppq;
            if spacing <= 0.0 {
                return 0;
            }
            let length =
                f64::from(((run.b.0 - run.a.0).powi(2) + (run.b.1 - run.a.1).powi(2)).sqrt());
            (length / spacing).floor().max(0.0) as u32
        }
    }
}

fn recolour(channel: u8, index: usize, params: &CopyPathParams) -> u8 {
    match params.colours {
        ColourMode::Keep => channel,
        ColourMode::Shift { step } => {
            let value = i32::from(channel) + i32::from(step) * index as i32;
            value.rem_euclid(16) as u8
        }
        ColourMode::Turn => (channel + 1) % 16,
        ColourMode::One { colour } => colour.min(16),
    }
}

fn flip_axis(mode: FlipMode, dir: Dir, axis: FlipWay) -> bool {
    match mode {
        FlipMode::None => false,
        FlipMode::Mirror => true,
        FlipMode::Each { way } => dir == Dir::Down && (matches!(way, FlipWay::Both) || way == axis),
    }
}

fn bbox(notes: &[ShapeNote]) -> ShapeRect {
    let mut lo_t = f32::INFINITY;
    let mut hi_t = f32::NEG_INFINITY;
    let mut lo_k = f32::INFINITY;
    let mut hi_k = f32::NEG_INFINITY;
    for note in notes {
        lo_t = lo_t.min(note.tick);
        hi_t = hi_t.max(note.tick + note.length);
        lo_k = lo_k.min(note.key as f32);
        hi_k = hi_k.max(note.key as f32);
    }
    ShapeRect::normalized(lo_t, lo_k, hi_t, hi_k)
}

/// 便捷：由音符包围盒与路径点直接生成副本（不含原音符）
pub fn copies(notes: &[ShapeNote], params: &CopyPathParams, ppq: f64) -> Vec<ShapeNote> {
    apply(notes, params, ppq)
        .into_iter()
        .skip(notes.len())
        .collect()
}

/// 几何预览/命中用：把一条折线按 CopyAlong 参数复制（不含原折线）。
///
/// 与 `copy_notes` 同一套 placement；只把音符换成折线点，供组合镜像/预览
/// 覆盖副本范围（否则右键副本会落到 Notes 菜单）。
pub fn copies_of_points(
    points: &[(f32, f32)],
    params: &CopyPathParams,
    ppq: f64,
) -> Vec<Vec<(f32, f32)>> {
    if points.is_empty() || params.pts.is_empty() {
        return Vec::new();
    }
    let rect = bbox_points(points);
    let base = corner_a(&rect, Dir::Up);
    let mut out = Vec::new();
    for run in runs(params, base) {
        let count = count_for(&run, params, ppq);
        for place in place_on_run(&run, count, params.speed, params.size) {
            out.push(
                points
                    .iter()
                    .map(|&point| placed_point(point, &rect, &place, params))
                    .collect(),
            );
        }
    }
    out
}

fn bbox_points(points: &[(f32, f32)]) -> ShapeRect {
    let mut lo_t = f32::INFINITY;
    let mut hi_t = f32::NEG_INFINITY;
    let mut lo_k = f32::INFINITY;
    let mut hi_k = f32::NEG_INFINITY;
    for &(tick, key) in points {
        lo_t = lo_t.min(tick);
        hi_t = hi_t.max(tick);
        lo_k = lo_k.min(key);
        hi_k = hi_k.max(key);
    }
    ShapeRect::normalized(lo_t, lo_k, hi_t, hi_k)
}

fn placed_point(
    point: (f32, f32),
    rect: &ShapeRect,
    place: &Placement,
    params: &CopyPathParams,
) -> (f32, f32) {
    let width = (rect.tick_hi - rect.tick_lo).max(1e-6);
    let height = (rect.key_hi - rect.key_lo).max(1e-6);
    let mut local_x = point.0 - rect.tick_lo;
    if flip_axis(params.flip, place.dir, FlipWay::Time) {
        local_x = width - local_x;
    }
    let mut local_y = point.1 - rect.key_lo;
    if flip_axis(params.flip, place.dir, FlipWay::Keys) {
        local_y = height - local_y;
    }
    let x = place.corner.0 + local_x * place.scale[0];
    let y = match place.dir {
        Dir::Up => place.corner.1 + local_y * place.scale[1],
        Dir::Down => place.corner.1 - local_y * place.scale[1],
    };
    (x, y)
}

#[cfg(test)]
mod tests;
