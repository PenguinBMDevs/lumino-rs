//! 待确认形状的镜像：id、来源、所属模型与撤销快照。
//!
//! 结构体字段仍在 `drawn_shape.rs`，这里只放方法。

use super::super::*;

impl ShapeSelectState {
    /// 登记一个已确认图形（几何 + 它生成的音符），返回其稳定 ID
    pub fn add(
        &mut self,
        track: usize,
        group: Option<u64>,
        source: DrawnShapeSource,
        notes: Vec<ShapeNote>,
    ) -> u64 {
        self.push_shape(DrawnShape {
            id: self.next_id,
            model_id: None,
            track,
            group,
            source,
            notes,
            hidden_by_creation: false,
            deleted: false,
            ever_deleted: false,
            moves: Vec::new(),
            pending: None,
        })
    }

    /// 登记一个**待确认产物的镜像**（未 √ 的几何：无音符、无历史分组），返回其稳定 ID
    ///
    /// 两阶段交互下，未 √ 的曲线 / 形状 / 画刷在鼠标工具下照旧渲染（否则用户
    /// 「切过去图案就消失」），既然看得见就必须框得中、拖得动——本方法把它们纳入
    /// 选中域。音符仍只在 √ 时生成：镜像的 `notes` 恒空，移动 / 删除由调用方
    /// 写回 owning tool 的待确认几何（见 `Editor::translate_pending`）。
    pub fn add_pending(
        &mut self,
        track: usize,
        pending: PendingShapeRef,
        source: DrawnShapeSource,
    ) -> u64 {
        self.push_shape(DrawnShape {
            id: self.next_id,
            model_id: None,
            track,
            group: None,
            source,
            notes: Vec::new(),
            hidden_by_creation: false,
            deleted: false,
            ever_deleted: false,
            moves: Vec::new(),
            pending: Some(pending),
        })
    }

    /// 入表 + 自增 ID（`add` / `add_pending` 共用）
    fn push_shape(&mut self, shape: DrawnShape) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.shapes.push(shape);
        id
    }

    // ── 待确认镜像 ─────────────────────────────────────────

    /// 指定图形的待确认来源（已确认图形返回 `None`）
    pub fn pending_of(&self, id: u64) -> Option<PendingShapeRef> {
        self.shapes
            .iter()
            .find(|s| s.id == id)
            .and_then(|s| s.pending)
    }

    /// 某待确认来源对应的镜像 ID（无镜像返回 `None`）
    pub fn pending_mirror(&self, r: PendingShapeRef) -> Option<u64> {
        self.shapes
            .iter()
            .find(|s| s.pending == Some(r))
            .map(|s| s.id)
    }

    /// 覆写某图形的矢量几何（镜像几何跟随 owning tool 的待确认几何更新）。
    ///
    /// 唯一权威始终是 owning tool 的容器：镜像只是派生视图，故每次写回 / 平移后
    /// 都用「由 owning tool 重新导出」的几何覆写，杜绝两处几何各自演化而漂移。
    pub fn set_source(&mut self, id: u64, source: DrawnShapeSource) -> bool {
        let Some(s) = self.shapes.iter_mut().find(|s| s.id == id) else {
            return false;
        };
        s.source = source;
        true
    }

    /// 绑定/解绑 `ShapeStore` 模型 id
    pub fn set_model_id(&mut self, id: u64, model_id: Option<u64>) -> bool {
        let Some(shape) = self.shapes.iter_mut().find(|shape| shape.id == id) else {
            return false;
        };
        shape.model_id = model_id;
        true
    }

    /// 该图形绑定的模型 id
    pub fn model_id_of(&self, id: u64) -> Option<u64> {
        self.shapes
            .iter()
            .find(|shape| shape.id == id)
            .and_then(|shape| shape.model_id)
    }

    /// 记录 019–022 的还原快照
    pub fn set_restore(&mut self, id: u64, shapes: Vec<DrawnShape>) {
        self.restore.insert(id, shapes);
    }

    /// 取出还原快照（一次性）
    pub fn take_restore(&mut self, id: u64) -> Option<Vec<DrawnShape>> {
        self.restore.remove(&id)
    }

    /// 删除**全部**待确认镜像（离开鼠标工具时调用；已确认图形不受影响）
    pub fn remove_pending_all(&mut self) {
        self.shapes.retain(|s| s.pending.is_none());
        self.collapse_selection();
    }

    /// 待确认容器里删掉一件后，把同容器中**下标更大**的镜像前移一位
    ///
    /// 只有同容器（同一 `PendingShapeRef` 变体）且下标在被删元素之后的镜像会受
    /// 下标位移影响；几何本身不变，故无需重建镜像（ID / 选中集都保持稳定）。
    pub fn shift_pending_indices(&mut self, removed: PendingShapeRef) {
        for s in &mut self.shapes {
            let Some(p) = s.pending.as_mut() else {
                continue;
            };
            let (cur, gone) = match (p, removed) {
                (PendingShapeRef::Shape(i), PendingShapeRef::Shape(k)) => (i, k),
                (PendingShapeRef::CurvePath(i), PendingShapeRef::CurvePath(k)) => (i, k),
                (PendingShapeRef::BrushStroke(i), PendingShapeRef::BrushStroke(k)) => (i, k),
                (PendingShapeRef::Composed(i), PendingShapeRef::Composed(k)) => (i, k),
                _ => continue,
            };
            if *cur > gone {
                *cur -= 1;
            }
        }
    }

    /// 全部待确认镜像（几何 + 来源），按登记顺序
    pub fn pending_shapes(&self) -> impl Iterator<Item = (&DrawnShape, PendingShapeRef)> {
        self.shapes.iter().filter_map(|s| s.pending.map(|p| (s, p)))
    }
}
