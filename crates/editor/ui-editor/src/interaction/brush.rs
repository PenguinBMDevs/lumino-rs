//! 画刷工具绘制逻辑（矢量笔画两阶段交互）
//!
//! **第一阶段（按下拖动）**：只记录一笔**矢量笔画**（逻辑坐标折线：tick 自由浮点、
//! key 吸附整数），拖动期间**不写 document**；画布上以圆头圆尾粗线条实时预览
//! （见 `grid::brush_tool_box`）。
//! **第二阶段（松手待确认）**：笔画进入待确认集合，√ 一次性按覆盖范围生成音符
//! （按层写入分配音轨、一次历史记录），× 一次性丢弃；待确认笔画的编辑历史独立于
//! document 历史（`BrushToolState.path_history`）。
//!
//! 断墨修复：相邻采样点之间由 `brush_tool::cov::cover_cells` 做线段栅格化
//! （输出线段经过的**全部**网格单元），拖得再快也不会丢格；预览与生成共用该覆盖集。
//!
//! 命中优先级（按下）：**待确认笔画实心区 > 已确认音符 > 空白处新落笔**——
//! 笔画必须能被拖动，即使它盖在已有音符上方。
//!
//! Conductor 音轨（track 0）：禁止落笔（与曲线工具同源守卫，`pressed.rs` 亦有拦截）。

use crate::{Editor, HitType};
use iced_core::Point;

mod confirm;

/// 笔画实心区命中容差（像素）：线宽之外再放宽，降低"按不住"的挫败感
const STROKE_HIT_SLOP: f32 = 2.0;

/// 层颜色加深系数：`音符显示色 × 0.6` = 加深 40%（key 边界硬切换，不做渐变）
pub(crate) const LAYER_DARKEN: f32 = 0.6;

impl Editor {
    /// 画刷笔画总宽（像素）= 粗细度 × 单 key 高度（`zoom_y`）——随缩放实时重算
    pub(crate) fn brush_total_width_px(&self) -> f32 {
        let thickness = self.brush.thickness.max(1) as f32;
        (thickness * self.editor_state.view.zoom_y).max(1.0)
    }

    /// 某层分配音轨（doc 索引）
    ///
    /// - 显式配置：直接用 `BrushConfig.tracks[level]`。
    /// - 默认：以**落笔时刻的基准轨** `base_track` 为起点，沿普通音轨（排除
    ///   Conductor=0）序行走，每个 level 一条不同音轨；不足则循环复用。
    ///
    /// 用 `base_track` 而非"当前轨"：用户在"画完 → 切轨 → √"之间切换时，
    /// 音符落点必须与预览一致（预览也按同一 `base_track` 取色）。
    pub(crate) fn brush_track_for_level(&self, level: usize, base_track: usize) -> usize {
        if let Some(t) = self.brush.track_for_level(level) {
            return t;
        }
        let num = self
            .editor_state
            .data
            .document
            .as_ref()
            .map(|d| d.track_count())
            .unwrap_or(1);
        if num <= 1 {
            return 0; // 仅有指挥轨时兜底
        }
        let normal_count = num - 1; // 排除 conductor(0)
        let pos_in_normal = if base_track == 0 {
            0
        } else {
            (base_track - 1).min(normal_count - 1)
        };
        let idx = (pos_in_normal + level) % normal_count;
        1 + idx
    }

    /// 某层笔画颜色 = 该层音轨的**音符显示色**加深 40%（含洋葱皮轨，同一规则）
    ///
    /// 取色单源 = `current_track_color_f32(doc_track)`，与卷帘音符实例着色同源，
    /// 因此调色板变更后下一次重绘即变色（无需缓存失效）。
    pub(crate) fn brush_layer_color(&self, level: usize, base_track: usize) -> iced_core::Color {
        let track = self.brush_track_for_level(level, base_track);
        let c = lumino_extras::palette::current_track_color_f32(track);
        darken(
            iced_core::Color::from_rgba(c[0], c[1], c[2], c[3]),
            LAYER_DARKEN,
        )
    }

    /// 处理画刷工具按下：命中待确认笔画 → 拖动；命中音符 → 编辑；否则开始新笔画
    pub(crate) fn handle_brush_pressed(
        &mut self,
        pos: Point,
        hit_result: Option<(usize, HitType)>,
        key: u16,
    ) {
        // Conductor 音轨（track 0）：整工具不可用，不开始任何笔画（与曲线工具对齐）
        if self.editor_state.data.current_track == 0 {
            return;
        }
        // 优先级 1：待确认笔画实心区 → 整体拖动（X 自由、Y 按单个 key 吸附）
        if let Some(index) = self.brush_stroke_hit_test(pos) {
            let raw = (self.pos_to_tick(pos), self.pos_to_raw_key(pos));
            if self.editor_state.brush_tool.begin_drag(index, raw) {
                return;
            }
        }
        // 优先级 2：命中已确认音符 → 复用铅笔的「编辑已有音符」逻辑（保持既有行为）
        if let Some((index, hit_type)) = hit_result {
            self.start_note_edit(index, hit_type, pos);
            return;
        }
        // 优先级 3：空白处 → 开始新笔画（不清空已有待确认笔画）
        let base_track = self.editor_state.data.current_track;
        let raw_tick = self.pos_to_tick(pos);
        self.editor_state
            .brush_tool
            .begin_stroke((raw_tick, key as f32), base_track);
        // 新笔画占一步笔画历史（拖动过程不逐帧改历史，松手时合并到栈顶）
        self.editor_state.brush_tool.push_path_history();
    }

    /// 鼠标移动：落笔绘制中追加采样点；整体拖动中按增量平移
    ///
    /// **不写 document、不置任何脏标记**——预览由画布覆盖层实时绘制，
    /// 因此拖拽成本与笔画长度解耦（无逐格入库、无空间索引重建）。
    pub(crate) fn handle_brush_moved(&mut self, pos: Point) {
        let brush = &self.editor_state.brush_tool;
        if !brush.is_active() {
            return;
        }
        let raw_tick = self.pos_to_tick(pos);
        if brush.is_drawing() {
            // 落笔：key 按单个 key 吸附（`pos_to_key` 已四舍五入），tick 自由
            let key = self.pos_to_key(pos) as f32;
            self.editor_state.brush_tool.push_point((raw_tick, key));
        } else {
            // 整体拖动：X 向自由、Y 向按单个 key 吸附（`drag_to` 内部取整）
            let raw_key = self.pos_to_raw_key(pos);
            self.editor_state.brush_tool.drag_to((raw_tick, raw_key));
        }
    }

    /// 结束画刷笔触（释放时调用）：进入待确认状态
    ///
    /// 落笔结束把整笔合并进栈顶（新笔画 = 一步撤销）；拖动结束把位移记为一步撤销。
    /// 松手不写 document，不触发 `mark_notes_changed`（避免空间索引/洋葱皮无谓重建）。
    pub(crate) fn finish_brush_stroke(&mut self) {
        let brush = &mut self.editor_state.brush_tool;
        if brush.is_drawing() {
            brush.finish_stroke();
            brush.update_top_path_history();
        } else if brush.is_dragging() {
            brush.end_drag();
            brush.push_path_history();
        }
    }

    /// 笔画实心区命中测试（屏幕空间）：返回最上层（最后绘制）命中的笔画索引
    ///
    /// 实心区 = 折线在屏幕空间按半径 `总宽/2 + 容差` 膨胀后的区域；
    /// 单点笔画退化为圆。
    pub(crate) fn brush_stroke_hit_test(&self, pos: Point) -> Option<usize> {
        let brush = &self.editor_state.brush_tool;
        if !brush.has_pending() {
            return None;
        }
        let radius = self.brush_total_width_px() * 0.5 + STROKE_HIT_SLOP;
        for (index, stroke) in brush.strokes.iter().enumerate().rev() {
            match stroke.points.as_slice() {
                [] => continue,
                [only] => {
                    let p = self.line_pos_screen_pos(*only);
                    if (p.x - pos.x).hypot(p.y - pos.y) <= radius {
                        return Some(index);
                    }
                }
                points => {
                    for pair in points.windows(2) {
                        let a = self.line_pos_screen_pos(pair[0]);
                        let b = self.line_pos_screen_pos(pair[1]);
                        if point_segment_distance(pos, a, b) <= radius {
                            return Some(index);
                        }
                    }
                }
            }
        }
        None
    }

    /// 待确认笔画的屏幕包围盒 `(min_x, max_x, min_y, max_y)`（√× 按钮定位用）
    ///
    /// 先在**逻辑坐标**求包围盒，再映射四角——视图映射对 tick/key 单调，
    /// 四角映射即可得到精确屏幕包围盒，避免逐点做完整屏幕变换。
    pub(crate) fn brush_strokes_screen_bounds(&self) -> Option<(f32, f32, f32, f32)> {
        let mut logical: Option<(f32, f32, f32, f32)> = None;
        for stroke in &self.editor_state.brush_tool.strokes {
            for &(tick, key) in &stroke.points {
                logical = Some(match logical {
                    None => (tick, tick, key, key),
                    Some((t0, t1, k0, k1)) => {
                        (t0.min(tick), t1.max(tick), k0.min(key), k1.max(key))
                    }
                });
            }
        }
        let (t0, t1, k0, k1) = logical?;
        let half = self.brush_total_width_px() * 0.5;
        let corners = [
            self.line_pos_screen_pos((t0, k0)),
            self.line_pos_screen_pos((t0, k1)),
            self.line_pos_screen_pos((t1, k0)),
            self.line_pos_screen_pos((t1, k1)),
        ];
        let mut bounds = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for p in corners {
            bounds = (
                bounds.0.min(p.x - half),
                bounds.1.max(p.x + half),
                bounds.2.min(p.y - half),
                bounds.3.max(p.y + half),
            );
        }
        Some(bounds)
    }
}

/// 层颜色加深（"加深 40%" = 各通道 × 0.6，alpha 保持）
pub(crate) fn darken(color: iced_core::Color, factor: f32) -> iced_core::Color {
    iced_core::Color {
        r: (color.r * factor).clamp(0.0, 1.0),
        g: (color.g * factor).clamp(0.0, 1.0),
        b: (color.b * factor).clamp(0.0, 1.0),
        a: color.a,
    }
}

/// 点到线段的最短距离（屏幕像素）
fn point_segment_distance(p: Point, a: Point, b: Point) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len_sq = dx * dx + dy * dy;
    if len_sq <= f32::EPSILON {
        return (p.x - a.x).hypot(p.y - a.y);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq).clamp(0.0, 1.0);
    let proj = Point::new(a.x + t * dx, a.y + t * dy);
    (p.x - proj.x).hypot(p.y - proj.y)
}

#[cfg(test)]
mod tests;
