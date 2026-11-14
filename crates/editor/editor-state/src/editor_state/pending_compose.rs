//! Bake 前组合会话（DRAW-041C）
//!
//! 组合只发生在 ✓ 前：pending 矢量 → 组合结果仍是 pending（预览 + 可撤销），
//! ✓ 才写文档音符并保留只读模型。本模块只存数据，不碰文档历史。

use lumino_core::shape::{ShapeModel, ShapeOp};

use super::line_tool::LinePath;
use super::shape_ops::CutRef;
use super::shape_tool::ShapeInstance;
use crate::ShapeNote;

/// 参与组合的待确认来源
#[derive(Debug, Clone, PartialEq)]
pub enum PendingSource {
    /// 形状工具待确认图形
    Shape(ShapeInstance),
    /// 曲线工具待确认路径
    CurvePath(LinePath),
}

/// 组合类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposedKind {
    /// 019：多形状折叠为 Live shape
    Live,
    /// 021：多形状连接为一条曲线
    Join,
    /// 022：两形状门控合并
    Merge,
    /// 025：单形状沿直线切成两片
    Slice,
    /// 025：两形状饼干模（先选刀、后选面团；刀的音符当墙）
    Mould,
    /// 026：两形状生成首/步/末补间组
    Between,
    /// 027：单形状沿默认路径复制（`ShapeOp::CopyAlong`）
    CopyAlong,
    /// 028：单形状渐强力度包络（`ShapeOp::VelocityEnv`）
    Velocity,
    /// 020：单个待确认形状/曲线转成的可编辑单支组合
    ///
    /// 参数化实例（kind + rect）表达不了旋转，进入「编辑图形」前统一提升成模型；
    /// `sources` 保留原实例，`Ctrl+Z`（撤销组合）可还原回参数化形状。
    Single,
    /// DEBT-14：从形状抽屉 Insert 进来的待确认形状
    ///
    /// 与 `Single` 的区别只在来源：`drawer` 携带原图与插入参数，
    /// 右键「退回抽屉」时据此回填；✓ 仍是唯一提交点。
    Insert,
}

/// 组合结果中的一支（Slice 的两片 / Between 的首末与步）
#[derive(Debug, Clone, PartialEq)]
pub struct ComposedPart {
    /// 该支的模型（几何/算子齐备）
    pub model: ShapeModel,
    /// 预览音符；✓ 时写入文档
    pub notes: Vec<ShapeNote>,
    /// Slice/Mould 的片簿记：记住原形状与切片时轮廓；轮廓一变即失效为普通形状
    pub cut: Option<CutRef>,
}

impl ComposedPart {
    /// 无片簿记的普通组合分支
    pub fn plain(model: ShapeModel, notes: Vec<ShapeNote>) -> Self {
        Self {
            model,
            notes,
            cut: None,
        }
    }
}

/// 一次 Bake 前组合的结果；`sources` 用于撤销回 pending 矢量
#[derive(Debug, Clone, PartialEq)]
pub struct PendingComposed {
    /// 组合类型
    pub kind: ComposedKind,
    /// 组合后的各支（Live/Join/Merge/027/028 为单支，Slice 两支，Between 首末+步）
    pub results: Vec<ComposedPart>,
    /// 参与组合的 pending 形状/路径（按组合前顺序）
    pub sources: Vec<PendingSource>,
    /// 抽屉插入的来源（仅 [`ComposedKind::Insert`] 有；「退回抽屉」据此回填）
    pub drawer: Option<lumino_core::shape::DrawerInsert>,
    /// 饼干模切口：空隙区间 `(key, start, end)`（文档坐标，DEBT-17）
    ///
    /// 刀在切完即被清除，切口靠本字段保留到 ✓ / ×；随组合结果一起平移。
    pub mould_cuts: Vec<(u16, f32, f32)>,
}

impl PendingComposed {
    /// 单支组合结果
    pub fn single(
        kind: ComposedKind,
        model: ShapeModel,
        notes: Vec<ShapeNote>,
        sources: Vec<PendingSource>,
    ) -> Self {
        Self {
            kind,
            results: vec![ComposedPart::plain(model, notes)],
            sources,
            drawer: None,
            mould_cuts: Vec::new(),
        }
    }

    /// 抽屉插入产生的待确认形状（DEBT-14）
    ///
    /// `Insert` 只是提案：不写文档、不推历史；✓ 时由 `confirm_shape_tool` 统一提交。
    pub fn from_drawer(
        model: ShapeModel,
        notes: Vec<ShapeNote>,
        drawer: lumino_core::shape::DrawerInsert,
    ) -> Self {
        Self {
            kind: ComposedKind::Insert,
            results: vec![ComposedPart::plain(model, notes)],
            sources: Vec::new(),
            drawer: Some(drawer),
            mould_cuts: Vec::new(),
        }
    }

    /// 片簿记失效判定：轮廓改了就退化为普通形状（DRAW-025）
    ///
    /// 返回是否发生了失效（调用方可用于提示「片身份已清除」）。
    pub fn validate_cuts(&mut self) -> bool {
        let mut degraded = false;
        for part in &mut self.results {
            if let Some(cut) = &mut part.cut
                && !cut.validate_against(&part.model)
            {
                degraded = true;
            }
        }
        if degraded {
            // 轮廓已变：切口不再对应任何几何，连同刀痕一起清除。
            self.mould_cuts.clear();
        }
        degraded
    }

    /// 全部预览音符（按支顺序）
    pub fn notes(&self) -> impl Iterator<Item = &ShapeNote> {
        self.results.iter().flat_map(|part| part.notes.iter())
    }

    /// 预览音符总数
    pub fn note_count(&self) -> usize {
        self.results.iter().map(|part| part.notes.len()).sum()
    }

    /// 第一支模型（单支组合的快捷入口）
    pub fn model(&self) -> Option<&ShapeModel> {
        self.results.first().map(|part| &part.model)
    }

    /// 整体平移组合结果（预览 + 模型一起动；sources 已从工具容器摘除，无需回写）
    pub fn translate(&mut self, dtick: f32, dkey: f32) {
        for part in &mut self.results {
            translate_model(&mut part.model, dtick, dkey, 0);
            for note in &mut part.notes {
                *note = note.translated(dtick, dkey);
            }
        }
        // 切口随组合结果一起平移，否则移动图案后刀痕留在原处。
        for (key, start, end) in &mut self.mould_cuts {
            *start = (*start + dtick).max(0.0);
            *end = (*end + dtick).max(0.0);
            *key = (*key as f32 + dkey).clamp(0.0, 255.0).round() as u16;
        }
        // 抽屉来源的落位矩形跟随 pending 一起移动，否则「退回抽屉」记录的位置是旧的。
        if let Some(drawer) = &mut self.drawer {
            let rect = &mut drawer.spec.rect;
            rect.tick_lo = (rect.tick_lo + dtick).max(0.0);
            rect.tick_hi = (rect.tick_hi + dtick).max(0.0);
            rect.key_lo = (rect.key_lo + dkey).clamp(0.0, 255.0);
            rect.key_hi = (rect.key_hi + dkey).clamp(0.0, 255.0);
        }
    }
}

/// 平移模型几何（含 Live 的内嵌 strokes / from，深度上限防递归）
fn translate_model(model: &mut ShapeModel, dtick: f32, dkey: f32, depth: usize) {
    if depth > 8 {
        return;
    }
    for anchor in &mut model.geometry.anchors {
        anchor.pos = (
            (anchor.pos.0 + dtick).max(0.0),
            (anchor.pos.1 + dkey).clamp(0.0, 255.0),
        );
    }
    if let Some(rect) = &mut model.geometry.rect {
        rect.tick_lo = (rect.tick_lo + dtick).max(0.0);
        rect.tick_hi = (rect.tick_hi + dtick).max(0.0);
        rect.key_lo = (rect.key_lo + dkey).clamp(0.0, 255.0);
        rect.key_hi = (rect.key_hi + dkey).clamp(0.0, 255.0);
    }
    for op in &mut model.ops {
        match op {
            ShapeOp::Live(params) => {
                for stroke in &mut params.strokes {
                    for anchor in &mut stroke.anchors {
                        anchor.pos = (
                            (anchor.pos.0 + dtick).max(0.0),
                            (anchor.pos.1 + dkey).clamp(0.0, 255.0),
                        );
                    }
                }
                for from in &mut params.from {
                    translate_model(from, dtick, dkey, depth + 1);
                }
            }
            // CopyAlong 的路径点是文档绝对坐标；组合结果平移时 `grp` 必须同步，
            // 否则后续按模型再生成会把副本放回旧位置。
            ShapeOp::CopyAlong(params) => {
                if let Some(grp) = &mut params.grp {
                    grp[0] = (grp[0] + dtick).max(0.0);
                    grp[1] = (grp[1] + dkey).clamp(0.0, 255.0);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_core::shape::{CopyPathParams, ShapeKind, ShapeModel, ShapeOp};

    #[test]
    fn test_composed_kind_is_copy() {
        let kind = ComposedKind::Live;
        let copy = kind;
        assert_eq!(copy, ComposedKind::Live);
    }

    #[test]
    fn test_translate_moves_copy_along_group_anchor() {
        let mut model = ShapeModel::new(0, 1, ShapeKind::Polyline, 1.0);
        model.ops.push(ShapeOp::CopyAlong(CopyPathParams {
            pts: vec![[100.0, 0.0]],
            grp: Some([10.0, 20.0]),
            ..CopyPathParams::default()
        }));
        let mut composed = PendingComposed {
            kind: ComposedKind::CopyAlong,
            results: vec![ComposedPart::plain(model, Vec::new())],
            sources: Vec::new(),
            drawer: None,
            mould_cuts: Vec::new(),
        };

        composed.translate(5.0, 2.0);

        let ShapeOp::CopyAlong(params) = &composed.results[0].model.ops[0] else {
            panic!("应为 CopyAlong 参数");
        };
        assert_eq!(
            params.grp,
            Some([15.0, 22.0]),
            "组合结果平移时 CopyAlong 的绝对路径起点必须同步"
        );
    }

    #[test]
    fn test_translate_moves_mould_cuts() {
        let model = ShapeModel::new(0, 1, ShapeKind::Polyline, 1.0);
        let mut composed = PendingComposed {
            kind: ComposedKind::Mould,
            results: vec![ComposedPart::plain(model, Vec::new())],
            sources: Vec::new(),
            drawer: None,
            mould_cuts: vec![(60, 100.0, 200.0)],
        };

        composed.translate(5.0, 2.0);

        assert_eq!(
            composed.mould_cuts,
            vec![(62, 105.0, 205.0)],
            "切口必须随组合结果一起平移"
        );
    }
}
