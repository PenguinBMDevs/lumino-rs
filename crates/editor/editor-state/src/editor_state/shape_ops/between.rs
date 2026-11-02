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
/// 返回的 `Vec<ShapeModel>` 中，每个组按 `First, Step/Key..., Last` 排列；
/// 原有 Step 被替换，Key 保留。手改过的 Step（指纹不匹配）会降级为 Key，
/// 后续不再被重算；周围的 Step 以相邻锚点（First / Key / Last）重新插值。
/// 缺失首/末的组会被解绑并给出 warning。
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

        // 手改过的 Step 降级为 Key：保留几何，不参与重算。
        let mut keys = Vec::new();
        for shape in shapes
            .iter_mut()
            .filter(|shape| group_id(shape) == Some(id))
        {
            match role_of(shape, id) {
                Some(BetweenRole::Step) if is_modified(shape) => {
                    if let Some(ShapeOp::Between(params)) = shape
                        .ops
                        .iter_mut()
                        .find(|op| matches!(op, ShapeOp::Between(p) if p.id == id))
                    {
                        params.role = BetweenRole::Key;
                    }
                    keys.push(shape.clone());
                }
                Some(BetweenRole::Key) => keys.push(shape.clone()),
                _ => {}
            }
        }
        keys.sort_by(|a, b| at_of(a, id).total_cmp(&at_of(b, id)));

        // 锚点：First / Key... / Last，按 at 排序；Key 落在 0/1 时跳过。
        let mut anchors: Vec<(f32, ShapeModel)> = Vec::with_capacity(keys.len() + 2);
        anchors.push((0.0, first.clone()));
        for key in &keys {
            let at = at_of(key, id);
            if at > 0.0 && at < 1.0 {
                anchors.push((at, key.clone()));
            }
        }
        anchors.push((1.0, last.clone()));
        anchors.sort_by(|a, b| a.0.total_cmp(&b.0));

        let base_id = shapes.iter().map(|shape| shape.id).max().unwrap_or(0) + 1;
        let mut generated: Vec<(f32, ShapeModel)> = Vec::with_capacity(steps as usize);
        for (offset, index) in (1..=steps).enumerate() {
            let at = f32::from(index) / f32::from(steps + 1);
            // Key 占住某个步位时不再生成同名步。
            if keys.iter().any(|key| (at_of(key, id) - at).abs() <= 1e-4) {
                continue;
            }
            let model = if keys.is_empty() {
                blend(&first, &last, graph_at(&set, at), set.rev)
            } else {
                let Some((a, b)) = bracket(&anchors, at) else {
                    continue;
                };
                let local = ((at - a.0) / (b.0 - a.0).max(1e-6)).clamp(0.0, 1.0);
                blend(&a.1, &b.1, local, set.rev)
            };
            let mut model = model;
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
            generated.push((at, model));
        }

        let mut members: Vec<(f32, ShapeModel)> = keys
            .into_iter()
            .filter_map(|key| {
                let at = at_of(&key, id);
                (at > 0.0 && at < 1.0).then_some((at, key))
            })
            .chain(generated)
            .collect();
        members.sort_by(|a, b| a.0.total_cmp(&b.0));

        let original = std::mem::take(shapes);
        let mut out = Vec::with_capacity(original.len() + members.len());
        for shape in original {
            if group_id(&shape) == Some(id) {
                match role_of(&shape, id) {
                    Some(BetweenRole::First) => {
                        out.push(shape);
                        out.extend(members.drain(..).map(|(_, model)| model));
                    }
                    Some(BetweenRole::Step) | Some(BetweenRole::Key) => {}
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

fn at_of(shape: &ShapeModel, id: u64) -> f32 {
    shape
        .ops
        .iter()
        .find_map(|op| match op {
            ShapeOp::Between(p) if p.id == id => p.at,
            _ => None,
        })
        .unwrap_or(0.5)
        .clamp(0.0, 1.0)
}

/// 找到 `at` 两侧的锚点（`anchors` 已按 at 排序且至少含 0/1）
fn bracket(
    anchors: &[(f32, ShapeModel)],
    at: f32,
) -> Option<((f32, ShapeModel), (f32, ShapeModel))> {
    for pair in anchors.windows(2) {
        let a = &pair[0];
        let b = &pair[1];
        if at >= a.0 && at <= b.0 {
            return Some((a.clone(), b.clone()));
        }
    }
    None
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
