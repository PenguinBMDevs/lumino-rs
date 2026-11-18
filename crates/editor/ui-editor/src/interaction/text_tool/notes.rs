//! 占用网格 → 音符列表（纯函数，与字体/画布无关）

use super::rasterize::GlyphGrid;

/// 纯函数：将占用网格按 [`GlyphGrid`] 的轴角色转换为音符列表（不依赖字体 / 画布）。
///
/// `merged = false`（正常采样）：每个有墨水的采样格生成一个音符，长度 = snap；
/// `merged = true`（key 范围合并）：**沿时间轴**把连续墨水合并为一个音符，
/// 长度 = 该段的时间跨度（横向 = 同一 key 行内的连续列；纵向 = 同一 key 列内的连续行），
/// 任意空隙断开，不合并本应分开的笔画。
///
/// 合并方向恒为**时间轴**：若照搬"按行合并"到纵向，合并方向会变成音高轴，于是
/// "音高跨度"会被当成"音符长度"写进文档（长度单位错乱），故合并方向必须由 `grid` 决定。
pub(crate) fn sample_to_notes_grid(
    occ: &[Vec<bool>],
    grid: &GlyphGrid,
    snap: f32,
    merged: bool,
) -> Vec<(f32, u16, f32)> {
    let mut notes = Vec::new();
    let rows = occ.len();
    let cols = occ.first().map_or(0, Vec::len);
    if rows == 0 || cols == 0 {
        return notes;
    }

    // 正常采样：逐格生成，长度 = snap（与方向无关）
    if !merged {
        for (r, row) in occ.iter().enumerate() {
            for (c, &on) in row.iter().enumerate() {
                if on {
                    let (tick, key) = grid.cell(r, c);
                    notes.push((tick, key, snap));
                }
            }
        }
        return notes;
    }

    // 合并采样：外层遍历**音高格**（横向 = 行、纵向 = 列），内层按**时间递增**遍历时间格。
    // 纵向的时间格是行号且 tick 随行号递减，故时间递增 = 行号倒序。
    let pitch_cells = if grid.vertical { cols } else { rows };
    let time_cells = if grid.vertical { rows } else { cols };
    for p in 0..pitch_cells {
        let mut run_start: Option<usize> = None; // 扫描顺序中首个时间格
        let mut run_last = 0usize;
        for k in 0..time_cells {
            let t = if grid.vertical { time_cells - 1 - k } else { k };
            let on = if grid.vertical {
                occ.get(t)
                    .and_then(|row| row.get(p))
                    .copied()
                    .unwrap_or(false)
            } else {
                occ.get(p)
                    .and_then(|row| row.get(t))
                    .copied()
                    .unwrap_or(false)
            };
            if on {
                if run_start.is_none() {
                    run_start = Some(t);
                }
                run_last = t;
            } else if let Some(start) = run_start.take() {
                push_time_run(&mut notes, grid, p, start, run_last, snap);
            }
        }
        if let Some(start) = run_start {
            push_time_run(&mut notes, grid, p, start, run_last, snap);
        }
    }
    notes
}

/// 时间轴合并段 `[start, last]`（扫描顺序，`start` 为段内**最早**时间格）→ 一条音符
fn push_time_run(
    notes: &mut Vec<(f32, u16, f32)>,
    grid: &GlyphGrid,
    pitch_cell: usize,
    start: usize,
    last: usize,
    snap: f32,
) {
    let len = (last.abs_diff(start) + 1) as f32 * snap;
    let key = grid.key_of_pitch_cell(pitch_cell);
    let tick = grid.tick_of_time_cell(start);
    notes.push((tick, key, len));
}
