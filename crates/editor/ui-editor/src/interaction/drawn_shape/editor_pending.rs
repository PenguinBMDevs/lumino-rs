//! 待确认形状的镜像与命中：写回几何、屏幕拾取、右键菜单准入。
//!
//! 方法仍在 `impl Editor`，只是按职责放在子模块。

use super::*;

impl Editor {
    /// 最近一次音符创建历史的**分组 ID**（`push_note_create` 之后调用）。
    ///
    /// 图形对象以此与历史条目绑定：撤销该条目时同组图形一并隐藏，
    /// 重做时恢复。非 Create 条目（或栈顶非 Create）返回 `None`。
    pub(crate) fn last_create_group(&self) -> Option<u64> {
        match self.editor_state.data.history.undo_back() {
            Some(HistoryEntry::Create(entry)) => entry.group_id,
            _ => None,
        }
    }

    /// DRAW-041B：Bake 后保留**只读 ShapeModel**（不登记可拖拽 DrawnShape）
    ///
    /// 模型带本次写出的音符指纹，供 032B/033 的 ShapeStore 列表入口使用；普通 Bake
    /// 仍然不产生可交互图形，保持 DRAW-041 的“确认后只留音符”语义。
    pub(crate) fn record_baked_shape_models(
        &mut self,
        models: Vec<(lumino_core::shape::ShapeModel, Vec<ShapeNote>)>,
    ) {
        let group = self.last_create_group().unwrap_or(0);
        for (model, notes) in models {
            let id = self.editor_state.shape_store.insert(model, group);
            self.editor_state.shape_store.assign_notes(id, notes);
        }
    }

    // ── 待确认产物镜像（未 √ 的几何进入图形选中域） ──────────
    ///
    /// `per_stroke_notes[i]` 与 `brush_tool.strokes[i]` 对应；图形以**落笔基准轨**
    /// （`BrushStroke::base_track`）登记，保证「在哪个轨画的就在哪个轨选中」。
    ///
    /// 当前 Bake 不再登记，仅保留测试入口验证记录语义；生产路径不应调用。
    #[cfg(test)]
    pub(crate) fn record_brush_strokes(&mut self, per_stroke_notes: Vec<Vec<ShapeNote>>) {
        let fallback = self.editor_state.data.current_track;
        let data: Vec<StrokeRecord> = self
            .editor_state
            .brush_tool
            .strokes
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.points.is_empty())
            .map(|(i, s)| {
                (
                    if s.base_track == 0 {
                        fallback
                    } else {
                        s.base_track
                    },
                    s.points.clone(),
                    per_stroke_notes.get(i).cloned().unwrap_or_default(),
                )
            })
            .collect();
        for (track, points, notes) in data {
            record_drawn_shape_on(self, track, DrawnShapeSource::Polyline { points }, notes);
        }
    }

    // ── 待确认产物镜像（未 √ 的几何进入图形选中域） ──────────

    /// 重建「待确认产物」镜像（幂等）
    ///
    /// **为什么必须有镜像**：曲线 / 形状 / 画刷是两阶段交互——拖动只产生待确认几何，
    /// 按 √ 才生成音符；而鼠标工具（框选工具）下这些几何**照旧渲染**
    /// （`Editor::pending_preview_visible`，否则用户「切过去图案就消失」）。
    /// 既然看得见，就必须框得中、拖得动：本方法把每件待确认几何镜像成图形选中域
    /// 条目，供框选命中、选中高亮、选框与拖动复用同一条链路。
    ///
    /// **音符仍只在 √ 时生成**：镜像 `notes` 恒空，移动 / 删除写回 owning tool 的
    /// 待确认容器（见 [`Self::translate_pending`] / [`Self::remove_pending`]），
    /// 文档与历史不受影响。
    ///
    /// 调用时机：切换到鼠标工具（进入选中域）、装载新文档（注册表被清空后）。
    /// 非鼠标工具时清空全部镜像：其它工具下由 owning 预览负责渲染，无需选中域条目。
    pub fn rebuild_pending_mirrors(&mut self) {
        // 重建会换掉镜像 ID：先记下当前选中的待确认来源，重建后按来源恢复选中态。
        // （`set_tool` 可能被同工具重复调用——面板条目重复点击等——重复点击不该清空选区。）
        let keep: Vec<PendingShapeRef> = self
            .editor_state
            .shape_select
            .selected_shapes()
            .filter_map(|s| s.pending)
            .collect();
        self.editor_state.shape_select.remove_pending_all();
        if self.editor_state.current_tool() != Tool::ShapeSelect {
            return;
        }
        let track = self.editor_state.data.current_track;
        // Conductor 轨（track 0）：各 owning 预览与工具本身都不可用（不渲染），
        // 镜像自然不建——「看得见」是「框得着」的前提，看不见就不该凭空可选中。
        if track == 0 {
            return;
        }
        let mut items: Vec<(PendingShapeRef, DrawnShapeSource)> = Vec::new();
        for i in 0..self.editor_state.shape_tool.shapes.len() {
            let r = PendingShapeRef::Shape(i);
            if let Some(src) = self.pending_source(r) {
                items.push((r, src));
            }
        }
        for i in 0..self.editor_state.line_tool.paths.len() {
            let r = PendingShapeRef::CurvePath(i);
            if let Some(src) = self.pending_source(r) {
                items.push((r, src));
            }
        }
        for i in 0..self.editor_state.brush_tool.strokes.len() {
            let r = PendingShapeRef::BrushStroke(i);
            if let Some(src) = self.pending_source(r) {
                items.push((r, src));
            }
        }
        for i in 0..self.editor_state.pending_composed.len() {
            let r = PendingShapeRef::Composed(i);
            if let Some(src) = self.pending_source(r) {
                items.push((r, src));
            }
        }
        for (r, src) in items {
            self.editor_state.shape_select.add_pending(track, r, src);
        }
        for r in keep {
            if let Some(id) = self.editor_state.shape_select.pending_mirror(r) {
                self.editor_state.shape_select.add_to_selection(id);
            }
        }
    }

    /// 待确认几何的矢量来源（**镜像几何的唯一权威 = owning tool 的待确认容器**）
    ///
    /// 每次写回 / 平移后都用本方法重新导出并覆写镜像几何，杜绝「镜像与 owning
    /// 各自演化」的漂移。曲线按与 √ 登记同源的 `flatten_path` 展平（同一口径）。
    pub(crate) fn pending_source(&self, r: PendingShapeRef) -> Option<DrawnShapeSource> {
        match r {
            PendingShapeRef::Shape(i) => {
                let s = self.editor_state.shape_tool.shapes.get(i)?;
                Some(DrawnShapeSource::Shape {
                    kind: s.kind,
                    rect: s.rect,
                    shift_constrained: s.shift_constrained,
                    filled: s.filled,
                    apex_high: s.apex_high,
                })
            }
            PendingShapeRef::CurvePath(i) => {
                let path = self.editor_state.line_tool.paths.get(i)?;
                Some(DrawnShapeSource::Polyline {
                    points: flatten_path(path)
                        .into_iter()
                        .map(|(t, k)| (t as f32, k as f32))
                        .collect(),
                })
            }
            PendingShapeRef::BrushStroke(i) => {
                let s = self.editor_state.brush_tool.strokes.get(i)?;
                Some(DrawnShapeSource::Polyline {
                    points: s.points.clone(),
                })
            }
            PendingShapeRef::Composed(i) => {
                let composed = self.editor_state.pending_composed.get(i)?;
                Some(DrawnShapeSource::PolylineSet {
                    polylines: composed
                        .results
                        .iter()
                        .flat_map(|part| {
                            polylines_of_model(&part.model, f32::from(self.editor_state.view.ppq))
                        })
                        .collect(),
                })
            }
        }
    }

    /// 把镜像几何回灌为 owning tool 的**当前**待确认几何（镜像 ID / 选中态不变）
    ///
    /// 待确认几何的唯一权威是 owning tool 的容器，镜像几何只是派生缓存。凡在鼠标工具
    /// 激活时可能改动待确认几何的入口，都必须回灌一次，否则镜像与预览 / 选框分叉，
    /// 重新落回「看得见却框不中 / 框与图案错位」这一类 BUG：
    /// - 镜像自身的拖动写回（[`Self::sync_pending_drag_geometry`]）；
    /// - **撤销 / 重做待确认路径编辑**（`Editor::undo/redo` 的 brush/line `*_path` 分支）——
    ///   该入口与工具无关，鼠标工具下按 Ctrl+Z 同样会回退待确认几何。
    /// - owning 工具自身的交互（拖动锚点/路径）与 √/×：这些入口都只在 owning 工具激活时
    ///   可达，而离开鼠标工具时镜像已被撤掉，无需回灌。
    ///
    /// owning 几何已整体消失（例如撤销回退了整条路径）→ 镜像一并退场，避免留下
    /// 「画不出来却框得中」的幽灵条目。
    pub(crate) fn refresh_pending_mirror_geometry(&mut self) {
        let mirrors: Vec<(u64, PendingShapeRef)> = self
            .editor_state
            .shape_select
            .pending_shapes()
            .map(|(s, r)| (s.id, r))
            .collect();
        for (id, r) in mirrors {
            match self.pending_source(r) {
                Some(src) => {
                    self.editor_state.shape_select.set_source(id, src);
                }
                None => {
                    self.editor_state.shape_select.remove(id);
                }
            }
        }
    }

    /// 屏幕坐标命中的待确认来源（命中已确认图形时回退到当前选中集里的 pending）。
    ///
    /// 与 [`Self::drawn_shape_pending_at_screen`] 的区别：这里返回具体来源，
    /// 供右键菜单判断「当前工具是不是该 pending 的 owner」。
    pub(crate) fn pending_ref_at_screen(&self, pos: Point) -> Option<PendingShapeRef> {
        if let Some(id) = self.drawn_shape_at_screen(pos)
            && let Some(pending) = self.editor_state.shape_select.pending_of(id)
        {
            return Some(pending);
        }
        // 点没命中形状本体（例如落在选框内的空白处）：
        // 只要该 pending 已被选中，右键仍应落到它身上——否则用户必须先框选
        // 或精确点中音符才能拿到长列表（DEBT-14 反馈）。
        let tick = self.pos_to_tick(pos);
        let key = self.pos_to_raw_key(pos);
        if !self.point_in_selection_box(tick, key) {
            return None;
        }
        self.editor_state
            .shape_select
            .selected_shapes()
            .find_map(|shape| shape.pending)
    }

    /// 当前工具是否应暴露待确认右键菜单。
    ///
    /// 选择工具对所有 pending 开放；形状工具只对形状/组合结果开放。
    /// 曲线/画笔的 pending 组合在 ShapeSelect 下完成（组合预览与 ✓× 由形状预览层承载），
    /// 避免在 owner 工具里生成看不见、也无法确认的 composed 结果。
    pub(crate) fn pending_menu_allowed(&self, pending: PendingShapeRef) -> bool {
        match self.editor_state.current_tool() {
            Tool::ShapeSelect => true,
            Tool::Shape => matches!(
                pending,
                PendingShapeRef::Shape(_) | PendingShapeRef::Composed(_)
            ),
            _ => false,
        }
    }

    /// 右键命中的图形是否属于“待确认”菜单目标。
    ///
    /// 返回：
    /// - `Some(true)`：命中 pending 镜像，或当前选中集含 pending（混合选区也按 pending 处理）；
    /// - `Some(false)`：命中已确认图形且选中集全是已确认图形；
    /// - `None`：没有命中图形。
    ///
    /// 这样右键菜单不会把 019–028 暴露给没有 `ShapeStore` 模型 / 音符的 pending 产物。
    pub fn drawn_shape_pending_at_screen(&self, pos: lumino_ui_core::geom::Point) -> Option<bool> {
        let id = self.drawn_shape_at_screen(pos)?;
        let hit_pending = self.editor_state.shape_select.pending_of(id).is_some();
        Some(hit_pending || self.has_pending_shape_selection())
    }

    // ── 选框（选中集外接框） ───────────────────────────────
}
