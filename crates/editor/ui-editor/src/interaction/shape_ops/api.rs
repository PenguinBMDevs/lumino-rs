//! 形状对象的加载/快照/分享入口（从 `shape_ops.rs` 拆出，控制文件长度）

use crate::Editor;
use crate::interaction::shape_bridge::model_from_drawn;
use lumino_core::shape::ShapeModel;

impl Editor {
    /// 打开工程后灌注形状：**只登记，不生成音符**（音符来自工程本身）
    pub fn load_shapes(&mut self, models: Vec<ShapeModel>) {
        self.editor_state.shape_store.load_all(models);
        self.mark_notes_changed();
    }

    /// 可持久化的形状快照（写工程用；隐藏/已删除的不写盘）
    pub fn shapes_snapshot(&self) -> Vec<ShapeModel> {
        self.editor_state.shape_store.persisted_models()
    }

    /// 当前选中的绘制图形对应的参数模型（DRAW-034 复制为文本用）。
    ///
    /// 只返回已绑定 `ShapeStore` 模型的图形；待确认镜像/旧无模型图形没有参数，
    /// 无法分享，直接跳过。
    pub fn selected_shape_models(&self) -> Vec<ShapeModel> {
        let selected = self.editor_state.shape_select.selected_ids();
        self.editor_state
            .shape_select
            .shapes()
            .iter()
            .filter(|shape| selected.contains(&shape.id))
            .filter_map(|shape| shape.model_id)
            .filter_map(|model_id| self.editor_state.shape_store.model(model_id).cloned())
            .collect()
    }

    /// 当前选中的**待确认图形**按当前几何构造一个临时 ShapeModel（DRAW-034）。
    ///
    /// 待确认镜像还没有 `ShapeStore` 模型，不能走 019–028 的参数入口；但按 √ 前的
    /// 几何本身可以分享，供用户先复制文本再决定是否 Bake。
    pub fn selected_pending_shape_model(&self) -> Option<ShapeModel> {
        let selected = self.editor_state.shape_select.selected_ids();
        let drawn = self
            .editor_state
            .shape_select
            .shapes()
            .iter()
            .find(|shape| selected.contains(&shape.id) && shape.pending.is_some())?;
        Some(model_from_drawn(drawn, 1.0))
    }

    /// 从一行分享文本粘贴形状到当前音轨（DRAW-034）。
    ///
    /// 解码按当前 PPQ 换算，形状 id 清零由 `create_shape` 重新分配；track 强制改为
    /// 当前轨，避免把分享者的音轨号带进来。返回实际创建数量。
    pub fn paste_shapes_from_text(&mut self, text: &str) -> Result<usize, String> {
        let track = self.editor_state.data.current_track;
        if track == 0 {
            return Err("指挥轨不能粘贴形状，请先选择普通音轨".to_string());
        }
        let ppq = self.editor_state.view.ppq;
        let shapes = lumino_core::shape::share::decode_shapes(text, ppq)
            .map_err(|error| error.to_string())?;
        if shapes.is_empty() {
            return Err("分享文本里没有形状".to_string());
        }

        let mut created = 0usize;
        for mut model in shapes {
            model.id = 0;
            model.track = track as u16;
            if self.create_shape(model) {
                created += 1;
            }
        }
        if created == 0 {
            return Err("形状未能创建（请检查当前音轨）".to_string());
        }
        Ok(created)
    }
}
