//! DRAW-025：切割布防（拖线切割）
//!
//! 右键「切割」→ 布防；在画布上拖出一条直线，松手即按该线切分唯一选中的
//! 待确认形状。Shift 关闭网格吸附。
//!
//! DEC-001：已 Bake 形状不提供任何切割入口（音符是唯一权威）。

use iced_core::Point;

use crate::Editor;

/// 切割布防状态
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SliceCutState {
    /// 起点（逻辑坐标 tick/key）
    pub from: (f32, f32),
    /// 当前终点（逻辑坐标）
    pub to: (f32, f32),
    /// 是否已按下（拖拽中）
    pub dragging: bool,
}

impl Editor {
    /// 布防：仅「唯一选中的待确认形状/曲线」可用
    pub fn begin_slice_cut(&mut self) -> bool {
        if !self.pending_single_selection() {
            return false;
        }
        self.slice_cut = Some(SliceCutState {
            from: (0.0, 0.0),
            to: (0.0, 0.0),
            dragging: false,
        });
        self.grid_cache.clear();
        true
    }

    /// 是否处于切割布防
    pub fn slice_cut_active(&self) -> bool {
        self.slice_cut.is_some()
    }

    /// 退出布防
    pub fn end_slice_cut(&mut self) {
        if self.slice_cut.take().is_some() {
            self.grid_cache.clear();
        }
    }

    /// 按下：记录起点（`shift` = 不吸附）
    pub(crate) fn slice_cut_pressed(&mut self, pos: Point, shift: bool) -> bool {
        if self.slice_cut.is_none() {
            return false;
        }
        let point = self.slice_logical(pos, shift);
        let Some(state) = &mut self.slice_cut else {
            return false;
        };
        state.from = point;
        state.to = point;
        state.dragging = true;
        self.grid_cache.clear();
        true
    }

    /// 拖动：更新终点（未按 Shift 时把方向吸附到 0°/45°/90°）
    pub(crate) fn slice_cut_moved(&mut self, pos: Point) -> bool {
        if !self.slice_cut.is_some_and(|state| state.dragging) {
            return false;
        }
        let shift = self.shift_pressed();
        let Some(state) = self.slice_cut else {
            return false;
        };
        let point = self.constrain_end(state.from, self.slice_logical(pos, shift), shift);
        let Some(state) = &mut self.slice_cut else {
            return false;
        };
        state.to = point;
        self.grid_cache.clear();
        true
    }

    /// 松手：按拖出的线切分；失败保留布防供重试
    ///
    /// 唯一选中已不是待确认形状时（选区被改），布防一并退出。
    pub(crate) fn slice_cut_released(&mut self) -> bool {
        let Some(state) = self.slice_cut else {
            return false;
        };
        if !state.dragging {
            return false;
        }
        if let Some(armed) = &mut self.slice_cut {
            armed.dragging = false;
        }
        self.grid_cache.clear();
        if self.compose_pending_slice(state.from, state.to) {
            self.slice_cut = None;
            true
        } else {
            false
        }
    }

    /// 屏幕坐标 → 逻辑坐标（不做网格吸附：切割线要的是角度，不是 beat 对齐）
    fn slice_logical(&self, pos: Point, _shift: bool) -> (f32, f32) {
        (
            self.pos_to_tick(pos).max(0.0),
            self.pos_to_raw_key(pos).clamp(0.0, 255.0),
        )
    }

    /// 方向约束：把拖拽向量吸附到 0°/45°/90°（屏幕空间）；Shift 关闭
    ///
    /// 网格吸附会把竖线端点拉到最近的 beat，导致切线变斜；切割要的是
    /// 可预期的方向，因此这里只约束角度，位置完全跟随鼠标。
    fn constrain_end(&self, start: (f32, f32), end: (f32, f32), shift: bool) -> (f32, f32) {
        if shift {
            return end;
        }
        let view = &self.editor_state.view;
        let dx = ((end.0 - start.0) * view.zoom_x).abs();
        let dy = ((end.1 - start.1) * view.zoom_y).abs();
        if dx <= 1.0 && dy <= 1.0 {
            return end;
        }
        // tan(22.5°)：落在 ±22.5° 内视为该轴方向
        if dy <= dx * 0.4142 {
            (end.0, start.1)
        } else if dx <= dy * 0.4142 {
            (start.0, end.1)
        } else {
            let dkey = (dx / view.zoom_y).max(0.0);
            let sign = if end.1 >= start.1 { 1.0 } else { -1.0 };
            (end.0, (start.1 + dkey * sign).clamp(0.0, 255.0))
        }
    }
}

#[cfg(test)]
mod tests;
