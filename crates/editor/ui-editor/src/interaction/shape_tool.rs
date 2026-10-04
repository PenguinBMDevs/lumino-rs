//! 形状工具拖拽绘制：矩形/圆/三角 拉出 → √ 批量确认生成音符
//!
//! 与 `line_tool` 同为「拖拽 → 实时预览 → √ 固化」，但纯拉框范式：形状在 √ 确认前
//! 为临时叠加，确认后「固化」为音符，不保存为矢量对象。
//! 填充桶（`fill_enabled`）决定确认时是否额外生成图形内部音符。
//!
//! **描边（轮廓，`filled = false`）与曲线工具的轮廓同源**：形状边界先导出为闭合折线
//! （`lumino_editor_state::shape_tool::shape_outline_path`），再交给曲线工具那一套
//! 蜘蛛网（Spiderweb）式逐音高行解析（`crate::interaction::line_tool::paths::path_notes`）
//! ——每个音高行一条音符、起点 = 进入该行的 tick、终点 = 下一条音符的起点 →
//! **无缝连奏、长度自然变化**，全程不使用吸附精度；闭合环从最左点重启、竖直段
//! 原本各占 1 tick（写入时按最小长度下限补齐到至少 128 分音符，见
//! [`crate::interaction::line_tool::paths::min_note_length_ticks`]），与 `line_tool/paths.rs`
//! 的轮廓口径完全一致。
//!
//! **填充（`filled = true`）不走这条路**：仍是「按 snap 网格枚举格点、每格一条定长
//! 音符」，开启「x 分音符」切分档位时按行合并连续格点后按全局网格切分——与曲线工具里
//! 「切分只作用于填充区间、轮廓走 `path_notes`」完全同构。
//!
//! 本文件只放**交互处理**（按下/拖动/释放 + 填充桶命中测试）；√/× 的生成逻辑在
//! [`confirm`]（含两条生成口径的完整说明），测试在 [`tests`]（三角形朝向回归在
//! [`tests_triangle`]）。

mod confirm;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_triangle;

use lumino_editor_state::shape_tool::point_in_shape;

use crate::Editor;

impl Editor {
    /// 形状工具：左键按下 —— 开始拖拽拉框
    ///
    /// - Conductor 轨道（track 0）：整工具不可用，直接返回；
    /// - 填充桶开启且点击命中某待确认图形内部：标记该图形为「已填充」
    ///   （支持用填充桶填充已拉出的图案），不开始新拖拽；
    /// - 否则开始拖拽拉框。
    ///
    /// `tick` / `key` 已是**调用方按 Shift 状态解析好的坐标**：
    /// Shift 按住时为鼠标原始浮点坐标（绕过 key/音符精度吸附，自由跟随鼠标），
    /// 否则为网格吸附后的坐标。正图形约束在 `effective_rect` 内基于该坐标计算。
    pub(crate) fn handle_shape_tool_pressed(&mut self, tick: f32, key: f32, _shift: bool) {
        // Conductor 音轨：形状工具不可用
        if self.editor_state.data.current_track == 0 {
            return;
        }
        // Ctrl+单击 = 打开「分音符填充」对话框（与曲线工具下的填充桶同行为），
        // 不标记填充、不开始拖拽。弹窗为主窗口覆盖层：只置请求位，由 Root 取走。
        if self.ctrl_pressed() {
            self.fill_division_dialog_requested = true;
            tracing::info!("形状工具: Ctrl+单击 → 请求打开分音符填充对话框");
            return;
        }
        // 填充桶：点击待确认图形内部 → 标记填充
        if self.editor_state.shape_tool.fill_enabled
            && let Some(idx) = self.shape_hit_test(tick, key)
        {
            self.editor_state.shape_tool.shapes[idx].filled = true;
            self.mark_notes_changed();
            return;
        }
        // 正常：开始拖拽拉框（坐标已由调用方解析：Shift=原始浮点 / 否则=网格吸附）
        self.editor_state.shape_tool.begin_drag((tick, key));
    }

    /// 形状工具：拖拽移动 —— 更新当前点（实时预览）
    pub(crate) fn handle_shape_tool_moved(&mut self, snapped_tick: f32, key: f32) {
        self.editor_state
            .shape_tool
            .update_drag((snapped_tick, key));
    }

    /// 形状工具：左键释放 —— 结束拖拽，生成待确认图形
    pub(crate) fn handle_shape_tool_released(&mut self) {
        let snap = self.editor_state.view.snap_precision;
        let shift = self.shift_pressed();
        if self.editor_state.shape_tool.end_drag(snap, shift).is_some() {
            self.mark_notes_changed();
        }
    }

    /// 命中待确认图形内部（用于填充桶点击填充），返回图形索引
    fn shape_hit_test(&self, tick: f32, key: f32) -> Option<usize> {
        let px_per_tick = self.editor_state.view.zoom_x;
        let px_per_key = self.editor_state.view.zoom_y;
        for (i, shape) in self.editor_state.shape_tool.shapes.iter().enumerate() {
            if point_in_shape(shape.spec(), px_per_tick, px_per_key, tick, key) {
                return Some(i);
            }
        }
        None
    }
}
