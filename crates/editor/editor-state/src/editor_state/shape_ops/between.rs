//! Add between 补间（DRAW-026）
//!
//! 首/末形状由用户画；`sync_groups` 在首/末变化后按组设置重算 Step。
//! 手改过的 Step 通过指纹降级为 Key，不再被覆盖。

use std::collections::BTreeSet;

use lumino_core::shape::{BetweenParams, BetweenRole, ShapeModel, ShapeOp};

pub mod blend;
pub mod graph;

pub use blend::blend;
pub use graph::graph_at;

/// 组同步提示
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BetweenWarning {
    /// 可读说明
    pub message: String,
}

/// 首/末变化后重算所有 Step
///
/// 返回的 `Vec<ShapeModel>` 中，每个组按 `First, Step..., Last` 排列；
/// 原有 Step 被替换，Key 保留。缺失首/末的组会被解绑并给出 warning。
pub fn sync_groups(shapes: &mut Vec<ShapeModel>) -> Vec<BetweenWarning> {
    let ids = shapes.iter().filter_map(group_id).collect::<BTreeSet<_>>();
    let mut warnings = Vec::new();
    for id in ids {
        let first = shapes
            .iter()
            .find(|shape| role_of(shape, id) == Some(BetweenRole::First))
            .cloned();
        let last = shapes
            .iter()
            .find(|shape| role_of(shape, id) == Some(BetweenRole::Last))
            .cloned();
        let (Some(first), Some(last)) = (first, last) else {
            for shape in shapes
                .iter_mut()
                .filter(|shape| group_id(shape) == Some(id))
            {
                shape
                    .ops
                    .retain(|op| !matches!(op, ShapeOp::Between(p) if p.id == id));
            }
            warnings.push(BetweenWarning {
                message: format!("Between 组 {id} 缺少 First/Last，已解绑"),
            });
            continue;
        };
        let set = first
            .ops
            .iter()
            .find_map(|op| match op {
                ShapeOp::Between(p) if p.id == id => p.set.clone(),
                _ => None,
            })
            .unwrap_or_default();
        let steps = set
            .steps
            .min(lumino_core::shape::params::between::MOST_STEPS);
        if set.steps > steps {
            warnings.push(BetweenWarning {
                message: format!("Between 组 {id} 的 steps 已钳到 {steps}"),
            });
        }
        let base_id = shapes.iter().map(|shape| shape.id).max().unwrap_or(0) + 1;
        let mut generated = Vec::with_capacity(steps as usize);
        for (offset, index) in (1..=steps).enumerate() {
            let at = f32::from(index) / f32::from(steps + 1);
            let f = graph_at(&set, at);
            let mut model = blend(&first, &last, f, set.rev);
            model.id = base_id + offset as u64;
            let params = BetweenParams {
                id,
                role: BetweenRole::Step,
                at: Some(at),
                sig: Some(fingerprint(&model)),
                push: None,
                vel: None,
                set: None,
            };
            model.ops = vec![ShapeOp::Between(params)];
            generated.push(model);
        }
        let original = std::mem::take(shapes);
        let mut out = Vec::with_capacity(original.len() + generated.len());
        for shape in original {
            if group_id(&shape) == Some(id) {
                match role_of(&shape, id) {
                    Some(BetweenRole::First) => {
                        out.push(shape);
                        out.append(&mut generated);
                    }
                    Some(BetweenRole::Step) => {}
                    _ => out.push(shape),
                }
            } else {
                out.push(shape);
            }
        }
        *shapes = out;
    }
    warnings
}

/// 制作指纹：不含绝对位置/时间；平移后不变
pub fn fingerprint(shape: &ShapeModel) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    mix(&mut hash, kind_code(shape.kind) as u64);
    mix(&mut hash, shape.geometry.anchors.len() as u64);
    let origin = shape
        .geometry
        .anchors
        .first()
        .map(|anchor| anchor.pos)
        .unwrap_or((0.0, 0.0));
    for anchor in &shape.geometry.anchors {
        mix(&mut hash, (anchor.pos.0 - origin.0).to_bits() as u64);
        mix(&mut hash, (anchor.pos.1 - origin.1).to_bits() as u64);
        mix(&mut hash, anchor.in_handle.0.to_bits() as u64);
        mix(&mut hash, anchor.in_handle.1.to_bits() as u64);
        mix(&mut hash, anchor.out_handle.0.to_bits() as u64);
        mix(&mut hash, anchor.out_handle.1.to_bits() as u64);
    }
    hash
}

/// Step 是否被手改（指纹不匹配）
pub fn is_modified(shape: &ShapeModel) -> bool {
    shape.ops.iter().any(|op| match op {
        ShapeOp::Between(p) if p.role == BetweenRole::Step => p.sig != Some(fingerprint(shape)),
        _ => false,
    })
}

fn group_id(shape: &ShapeModel) -> Option<u64> {
    shape.ops.iter().find_map(|op| match op {
        ShapeOp::Between(p) => Some(p.id),
        _ => None,
    })
}

fn role_of(shape: &ShapeModel, id: u64) -> Option<BetweenRole> {
    shape.ops.iter().find_map(|op| match op {
        ShapeOp::Between(p) if p.id == id => Some(p.role),
        _ => None,
    })
}

fn kind_code(kind: lumino_core::shape::ShapeKind) -> u8 {
    match kind {
        lumino_core::shape::ShapeKind::Line => 1,
        lumino_core::shape::ShapeKind::Polyline => 2,
        lumino_core::shape::ShapeKind::Freehand => 3,
        lumino_core::shape::ShapeKind::Curve => 4,
        lumino_core::shape::ShapeKind::Arc => 5,
        lumino_core::shape::ShapeKind::Custom => 6,
        lumino_core::shape::ShapeKind::Funnel => 7,
    }
}

fn mix(hash: &mut u64, value: u64) {
    *hash ^= value;
    *hash = hash.wrapping_mul(0x100000001b3);
}

#[cfg(test)]
mod tests;
