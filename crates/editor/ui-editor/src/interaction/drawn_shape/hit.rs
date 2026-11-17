use super::*;

/// 折线命中容差（屏幕像素）：点击位置到笔画折线的最短距离阈值
const HIT_TOLERANCE_PX: f32 = 6.0;

/// 形状命中外扩（屏幕像素）：贴边 / 零宽高的细图形也能选中
const SHAPE_HIT_SLOP_PX: f32 = 3.0;

/// 点到线段的最短距离（同坐标系）
pub(super) fn point_segment_distance(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let abx = b.0 - a.0;
    let aby = b.1 - a.1;
    let len2 = abx * abx + aby * aby;
    let t = if len2 <= f32::EPSILON {
        0.0
    } else {
        (((p.0 - a.0) * abx + (p.1 - a.1) * aby) / len2).clamp(0.0, 1.0)
    };
    let cx = a.0 + abx * t;
    let cy = a.1 + aby * t;
    ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt()
}

impl Editor {
    // ── 命中 ─────────────────────────────────────────────

    /// 命中当前音轨**最上层**（后登记优先）的可见图形，返回其 ID
    pub(crate) fn hit_test_drawn_shape(&self, tick: f32, key: f32) -> Option<u64> {
        let track = self.editor_state.data.current_track;
        let px_per_tick = self.editor_state.view.zoom_x;
        let px_per_key = self.editor_state.view.zoom_y;
        let probe = self.line_pos_screen_pos((tick, key));

        // 逆序遍历：最后登记的图形画在最上层，命中也应优先
        for shape in self.editor_state.shape_select.visible_on(track).rev() {
            // DEBT-14：待确认组合（含抽屉 Insert）按「预览音符覆盖区」命中。
            // 只测轮廓会漏掉填充图形内部——用户看到的是音符块，点在块内就必须选中。
            if let Some(lumino_editor_state::PendingShapeRef::Composed(index)) = shape.pending
                && self.composed_notes_hit(probe, index)
            {
                return Some(shape.id);
            }
            let hit = match &shape.source {
                DrawnShapeSource::Shape {
                    kind,
                    rect,
                    shift_constrained,
                    apex_high,
                    ..
                } => shape_hit_with_slop(
                    ShapeSpec {
                        kind: *kind,
                        rect: *rect,
                        shift_constrained: *shift_constrained,
                        apex_high: *apex_high,
                    },
                    px_per_tick,
                    px_per_key,
                    tick,
                    key,
                ),
                DrawnShapeSource::Polyline { points } => polyline_hit(self, probe, points),
                DrawnShapeSource::PolylineSet { polylines } => polylines
                    .iter()
                    .any(|points| polyline_hit(self, probe, points)),
            };
            if hit {
                return Some(shape.id);
            }
        }
        None
    }

    /// 屏幕坐标处命中的已绘制图形 ID（供画布右键判定菜单目标）
    pub fn drawn_shape_at_screen(&self, pos: lumino_ui_core::geom::Point) -> Option<u64> {
        let tick = self.pos_to_tick(pos);
        let key = self.pos_to_raw_key(pos);
        self.hit_test_drawn_shape(tick, key)
    }
}

impl Editor {
    /// 待确认组合的「预览音符覆盖区」命中（DEBT-14）
    ///
    /// 抽屉 Insert 的预览是一块块音符，用户点的是音符而不是细轮廓；
    /// 只按轮廓做 6px 容差会在填充图形内部完全打不中。
    fn composed_notes_hit(&self, probe: lumino_ui_core::geom::Point, index: usize) -> bool {
        /// 音符块外扩（逻辑像素），避免边缘一两像素的抖动漏选
        const PAD: f32 = 2.0;
        let Some(composed) = self.editor_state.pending_composed.get(index) else {
            return false;
        };
        composed.notes().any(|note| {
            let a = self.line_pos_screen_pos((note.tick, f32::from(note.key)));
            let b = self
                .line_pos_screen_pos((note.tick + note.length.max(1.0), f32::from(note.key) + 1.0));
            let (x0, x1) = (a.x.min(b.x) - PAD, a.x.max(b.x) + PAD);
            let (y0, y1) = (a.y.min(b.y) - PAD, a.y.max(b.y) + PAD);
            probe.x >= x0 && probe.x <= x1 && probe.y >= y0 && probe.y <= y1
        })
    }
}

/// 形状命中：精确判定 + 细图形外扩
///
/// 只为**贴边 / 零宽高的矩形**（例如贴在 tick 0 的竖线）提供 3px 容差；
/// 圆形 / 三角形与正常矩形保持精确判定，避免「外接框角落也能点中」。
fn shape_hit_with_slop(
    spec: ShapeSpec,
    px_per_tick: f32,
    px_per_key: f32,
    tick: f32,
    key: f32,
) -> bool {
    if point_in_shape(spec, px_per_tick, px_per_key, tick, key) {
        return true;
    }
    if !matches!(spec.kind, ShapeKind::Rectangle) {
        return false;
    }
    let (x0, y0, x1, y1) = effective_rect(spec, px_per_tick, px_per_key);
    let width_px = (x1 - x0).abs() * px_per_tick;
    let height_px = (y1 - y0).abs() * px_per_key;
    let thin_x = width_px < SHAPE_HIT_SLOP_PX * 2.0;
    let thin_y = height_px < SHAPE_HIT_SLOP_PX * 2.0;
    if !thin_x && !thin_y {
        return false;
    }
    let eps_tick = if thin_x {
        SHAPE_HIT_SLOP_PX / px_per_tick.max(1e-6)
    } else {
        0.0
    };
    let eps_key = if thin_y {
        SHAPE_HIT_SLOP_PX / px_per_key.max(1e-6)
    } else {
        0.0
    };
    let expanded = ShapeSpec {
        kind: spec.kind,
        rect: (x0 - eps_tick, y0 - eps_key, x1 + eps_tick, y1 + eps_key),
        shift_constrained: false,
        apex_high: spec.apex_high,
    };
    point_in_shape(expanded, px_per_tick, px_per_key, tick, key)
}

/// 折线命中：探测点到折线各段的屏幕距离是否 ≤ 容差
fn polyline_hit(
    editor: &Editor,
    probe: lumino_ui_core::geom::Point,
    points: &[(f32, f32)],
) -> bool {
    if points.is_empty() {
        return false;
    }
    let screen: Vec<(f32, f32)> = points
        .iter()
        .map(|&(t, k)| {
            let p = editor.line_pos_screen_pos((t, k));
            (p.x, p.y)
        })
        .collect();
    let probe = (probe.x, probe.y);
    if screen.len() == 1 {
        let d = ((probe.0 - screen[0].0).powi(2) + (probe.1 - screen[0].1).powi(2)).sqrt();
        return d <= HIT_TOLERANCE_PX;
    }
    screen
        .windows(2)
        .any(|seg| point_segment_distance(probe, seg[0], seg[1]) <= HIT_TOLERANCE_PX)
}
