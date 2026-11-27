//! 形状选择与框选：命中、拖动预览偏移、按下/移动/松开。
//!
//! 方法仍在 `impl Editor`，只是按职责放在子模块。

use super::*;

impl Editor {
    /// 选中集合的**选框**（屏幕空间轴对齐矩形，含拖拽实时预览偏移）
    ///
    /// 多选时是**整组的外接框**；返回值已按四周 [`SHAPE_BOX_PADDING_PX`] 外扩，
    /// 是**唯一的选框口径**：
    /// - 渲染（`grid::drawn_shape_box`）用它画框与四角手柄；
    /// - 命中（[`Self::point_in_selection_box`]）用它判定「框内拖动」。
    ///
    /// 渲染与命中同源，杜绝「看到的框抓不住 / 抓得住的框看不见」。
    /// 当前音轨无选中、几何全为空（空折线）时返回 `None`。
    ///
    /// **预览偏移按件区分**：已确认图形走「幽灵」语义（文档未变，几何在原位，
    /// 选框需叠加拖拽偏移才跟手）；待确认镜像的几何已随拖动**即时落位**
    /// （见 [`Self::sync_pending_drag_geometry`]），再叠偏移就成了双倍位移。
    pub fn selection_box(&self) -> Option<Rectangle> {
        let track = self.editor_state.data.current_track;
        let (dtick, dkey) = self.drag_preview_delta();
        // 命中集在**屏幕空间**取并集：逻辑空间与屏幕不是线性同尺度（纵横缩放不同），
        // 先各自换算再合并，才能保证框住每个选中图形（含 padding 外扩）。
        let mut acc: Option<(f32, f32, f32, f32)> = None;
        for s in self.editor_state.shape_select.selected_shapes_on(track) {
            let Some(b) = s.source.bounds() else {
                continue;
            };
            let (dt, dk) = if s.pending.is_some() {
                (0.0, 0.0)
            } else {
                (dtick, dkey)
            };
            let a = self.line_pos_screen_pos((b.0 + dt, b.2 + dk));
            let c = self.line_pos_screen_pos((b.1 + dt, b.3 + dk));
            let r = (a.x.min(c.x), a.y.min(c.y), a.x.max(c.x), a.y.max(c.y));
            acc = Some(match acc {
                None => r,
                Some(p) => (p.0.min(r.0), p.1.min(r.1), p.2.max(r.2), p.3.max(r.3)),
            });
        }
        let (x0, y0, x1, y1) = acc?;
        Some(Rectangle::new(
            Point::new(x0 - SHAPE_BOX_PADDING_PX, y0 - SHAPE_BOX_PADDING_PX),
            Size::new(
                ((x1 - x0) + SHAPE_BOX_PADDING_PX * 2.0).max(1.0),
                ((y1 - y0) + SHAPE_BOX_PADDING_PX * 2.0).max(1.0),
            ),
        ))
    }

    /// 当前拖拽的实时预览偏移 `(dtick, dkey)`；未拖拽时为 `(0, 0)`
    ///
    /// 拖拽作用于**整个选中集合**，故这里是「整组共同偏移」——渲染侧对每个选中图形
    /// 统一套用，无需按图形区分。
    pub fn drag_preview_delta(&self) -> (f32, f32) {
        match self.editor_state.shape_select.drag() {
            Some(d) => (d.delta_tick, d.delta_key),
            None => (0.0, 0.0),
        }
    }

    /// 逻辑坐标点是否落在选中集的选框内（「框内拖动移动整组」的命中判定）
    pub(crate) fn point_in_selection_box(&self, tick: f32, key: f32) -> bool {
        let Some(rect) = self.selection_box() else {
            return false;
        };
        rect.contains(self.line_pos_screen_pos((tick, key)))
    }

    /// 选中一个绘制图形（供右键菜单等外部入口）
    ///
    /// 该图形**已在选中集内时保持多选不变**——使右键菜单的「删除」作用于既有批量选区，
    /// 与音符右键「命中已在选中集合内则不动选区」的语义一致；否则退化为唯一选中它。
    pub fn select_drawn_shape(&mut self, id: u64) {
        if !self.editor_state.shape_select.is_selected(id) {
            self.editor_state.shape_select.select_only(id);
        }
    }

    // ── 点选 / 拖动 ───────────────────────────────────────

    /// 鼠标工具：左键按下 —— 点选并起拖 / 框内起拖 / 空白起框
    ///
    /// `additive` = 按住修饰键（Shift）：命中图形时**切换**其选中态（加/减）；
    /// 空白起框时松手**并入**既有选中集而非替换。
    ///
    /// 拖动目标确定顺序（**统一点选与框选两种来源的移动方式**）：
    ///
    /// ① 命中某图形。`additive` → 切换该图形选中态（切换后仍选中才进入拖拽）；
    /// 否则它**不在**选中集内 → 以它为唯一选中（替换）；否则它**在**选中集内 →
    /// 保持整个选中集不变（于是「点中多选之一再拖」= 整组移动）。
    /// 最终只要它处于选中态就进入拖拽。
    ///
    /// ② 未命中图形，但落在**选中集选框内部** → 拖动整组（无需精确点中细线）。
    ///
    /// ③ 都不满足 → 空白按下：非 `additive` 时取消选中，并起框（是否构成框选由松手时的
    /// 屏幕尺度判定，见 [`Self::finish_shape_marquee`]）。
    pub(crate) fn handle_shape_select_pressed(&mut self, tick: f32, key: f32, additive: bool) {
        // 新一次拖动从零开始：已写回待确认几何的累计偏移必须归零，否则
        // `sync_pending_drag_geometry` 会按上一次的残值算出错误增量。
        self.shape_tools.reset_pending_drag_applied();
        // 手势边界自愈：命中判定与选框都建立在镜像几何之上，动手前先保证它是最新的
        // （任何未预料到的待确认几何改动都不会再退化成「框不中」）。
        self.refresh_pending_mirror_geometry();
        if self.begin_shape_edit(tick, key) {
            return;
        }
        // 020 编辑图形：锚点/旋转热区之外只处理「框内拖动」。
        //   单击不退出编辑态（拖动一次没抓准就丢状态，很难操作），
        //   退出走双击空白处或右键「编辑图形」再点一次。
        if self.is_shape_edit_armed() {
            if self.point_in_selection_box(tick, key) {
                self.editor_state.shape_select.begin_drag(tick, key);
            }
            return;
        }
        match self.hit_test_drawn_shape(tick, key) {
            Some(id) => {
                if additive {
                    self.editor_state.shape_select.toggle_selection(id);
                } else if !self.editor_state.shape_select.is_selected(id) {
                    self.editor_state.shape_select.select_only(id);
                }
                // 已 Bake 图形预先绑定模型，手柄才能在选中后立即显示/命中。
                if self.editor_state.shape_select.is_selected(id) {
                    let _ = self.ensure_model_for_drawn(id);
                    self.editor_state.shape_select.begin_drag(tick, key);
                }
            }
            None => {
                if !additive && self.point_in_selection_box(tick, key) {
                    // 选中集选框内部：拖动整组（选中集不变）
                    self.editor_state.shape_select.begin_drag(tick, key);
                } else {
                    if !additive {
                        self.editor_state.shape_select.clear_selection();
                    }
                    // 框选按鼠标原始 tick：吸附会把不足一拍的拉框压成一条竖线，
                    // 夹在两个 beat 之间的图形就永远框不中（DRAW-025 S4 实测）。
                    self.editor_state
                        .shape_select
                        .begin_marquee(tick, key, additive);
                }
            }
        }
        // 编辑图形绑定「当前这一个目标」：换选/取消选中后自动失效，
        // 避免下次选中同一对象时凭空回到编辑态。
        if !self.is_shape_edit_armed() {
            self.shape_tools.edit_armed = None;
            self.shape_tools.group_edit = None;
        }
        // 仅叠加层视觉变化（文档未变）：清网格缓存驱动重绘。
        // **不能**用 `mark_notes_changed()`——那会置 notes_changed 并重建空间索引（O(N)）。
    }

    /// 鼠标工具：拖动中 —— 更新预览偏移（拖动整组）或拉框矩形（框选），均不改文档
    pub(crate) fn handle_shape_select_moved(&mut self, tick: f32, key: f32) {
        if self.is_shape_editing() {
            self.update_shape_edit(tick, key);
            return;
        }
        if self.editor_state.shape_select.is_dragging() {
            let snap = self.editor_state.view.snap_precision;
            self.editor_state.shape_select.update_drag(tick, key, snap);
            // 待确认镜像即时落位（owning 预览 / 镜像描边 / 选框共用同一份几何）
            sync_pending_drag_geometry(self);
        } else if self.editor_state.shape_select.is_marqueeing() {
            // 与按下同口径：不吸附，避免把拉框压成竖线
            self.editor_state.shape_select.update_marquee(tick, key);
        }
    }

    /// 鼠标工具：左键释放 —— 有实际位移则提交**整组移动**，否则落在框选上
    pub(crate) fn handle_shape_select_released(&mut self) {
        if self.is_shape_editing() {
            self.finish_shape_edit();
            self.shape_tools.reset_pending_drag_applied();
            return;
        }
        if let Some((dtick, dkey)) = self.editor_state.shape_select.end_drag() {
            move_selected_shapes(self, dtick, dkey);
        } else if let Some(area) = self.editor_state.shape_select.end_marquee() {
            finish_shape_marquee(self, area);
        }
        self.shape_tools.reset_pending_drag_applied();
    }

    // ── 删除 ─────────────────────────────────────────────
}
