//! 形状几何：类型、锚点、外接框与统一点列
//!
//! 落盘纪律与模块划分见 [`super`] 的模块文档。

use serde::{Deserialize, Serialize};

/// 形状几何类型
///
/// 对应 Spiderweb `notes/engine.py` 的 `KINDS`
/// （`line / poly / free / curve / arc / custom / funnel`）。
/// 变体顺序是落盘契约，**只能追加**。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ShapeKind {
    /// 直线（两点）
    #[default]
    Line,
    /// 折线（多点）
    Polyline,
    /// 自由手绘（密集采样点）
    Freehand,
    /// 贝塞尔曲线（锚点 + 进出控制柄）
    Curve,
    /// 三点圆弧（起点 / 经过点 / 终点）
    Arc,
    /// 自定义形状 / 形状库图形（外接框 + 轮廓）
    Custom,
    /// 漏斗（线 + 墙）
    Funnel,
}

/// 锚点弯折行为（DRAW-042）
///
/// `Smooth` 为默认：自动柄按相邻段方向 1/3 维护；`Corner` 表示该锚点两侧
/// 都按直线处理，控制柄不显示也不参与命中。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AnchorBend {
    /// 曲线弯折（平滑通过）
    #[default]
    Smooth,
    /// 折线弯折（尖角）
    Corner,
}

/// 贝塞尔锚点
///
/// 位置与控制柄均为逻辑坐标 `(tick, key)`，`key` 用 `f32` 以支持不吸附网格的自由锚点。
/// 语义与 `lumino-editor-state` 的 `BezierAnchor` 一致：控制柄为相对锚点的偏移，
/// `handles_auto` 为真时由相邻段方向 1/3 长度自动维护（此时首尾相同时为精确直线）。
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct ShapeAnchor {
    /// 锚点位置（tick, key）
    pub pos: (f32, f32),
    /// 入向控制柄偏移（相对 `pos`，控制来自上一锚点的段）
    pub in_handle: (f32, f32),
    /// 出向控制柄偏移（相对 `pos`，控制到下一锚点的段）
    pub out_handle: (f32, f32),
    /// 控制柄是否自动维护（未被用户自定义）
    pub handles_auto: bool,
    /// 弯折行为（DRAW-042；v3 新增，旧分享文本缺省为 Smooth）
    #[serde(default)]
    pub bend: AnchorBend,
}

impl ShapeAnchor {
    /// 构造自动柄锚点
    pub fn new(pos: (f32, f32)) -> Self {
        Self {
            pos,
            in_handle: (0.0, 0.0),
            out_handle: (0.0, 0.0),
            handles_auto: true,
            bend: AnchorBend::Smooth,
        }
    }

    /// 设置出向控制柄（标记为自定义，不再自动维护）
    pub fn set_out_handle(&mut self, offset: (f32, f32)) {
        self.out_handle = offset;
        self.handles_auto = false;
    }

    /// 设置入向控制柄（标记为自定义，不再自动维护）
    pub fn set_in_handle(&mut self, offset: (f32, f32)) {
        self.in_handle = offset;
        self.handles_auto = false;
    }

    /// 交换入/出控制柄（路径反向时使用）
    pub fn swap_handles(&mut self) {
        std::mem::swap(&mut self.in_handle, &mut self.out_handle);
    }

    /// 出向控制柄绝对坐标
    pub fn out_handle_abs(&self) -> (f32, f32) {
        (
            self.pos.0 + self.out_handle.0,
            self.pos.1 + self.out_handle.1,
        )
    }

    /// 入向控制柄绝对坐标
    pub fn in_handle_abs(&self) -> (f32, f32) {
        (self.pos.0 + self.in_handle.0, self.pos.1 + self.in_handle.1)
    }

    /// 坐标是否全为有限值
    pub fn is_finite(&self) -> bool {
        let finite = |p: (f32, f32)| p.0.is_finite() && p.1.is_finite();
        finite(self.pos) && finite(self.in_handle) && finite(self.out_handle)
    }
}

/// 形状外接框（逻辑坐标）
///
/// 供圆 / 多边形 / 文字 / 自定义形状这类"框型"几何使用；
/// 只存规范化后的角度，**不存绘制方向**（方向属于 `ShapeKind` 语义，例如三角形朝向另立字段）。
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct ShapeRect {
    /// 时间轴下界（tick）
    pub tick_lo: f32,
    /// 音高下界（key）
    pub key_lo: f32,
    /// 时间轴上界（tick）
    pub tick_hi: f32,
    /// 音高上界（key）
    pub key_hi: f32,
}

impl ShapeRect {
    /// 构造并规范化（保证 lo <= hi）
    pub fn normalized(tick_a: f32, key_a: f32, tick_b: f32, key_b: f32) -> Self {
        Self {
            tick_lo: tick_a.min(tick_b),
            key_lo: key_a.min(key_b),
            tick_hi: tick_a.max(tick_b),
            key_hi: key_a.max(key_b),
        }
    }

    /// 是否已规范化
    pub fn is_normalized(&self) -> bool {
        self.tick_lo <= self.tick_hi && self.key_lo <= self.key_hi
    }

    /// 是否全为有限值
    pub fn is_finite(&self) -> bool {
        self.tick_lo.is_finite()
            && self.key_lo.is_finite()
            && self.tick_hi.is_finite()
            && self.key_hi.is_finite()
    }

    /// 宽（tick）与高（key）
    pub fn size(&self) -> (f32, f32) {
        (self.tick_hi - self.tick_lo, self.key_hi - self.key_lo)
    }
}

/// 形状几何：统一点列 + 可选外接框
///
/// 刻意**不用枚举**区分线/框/自由笔画：所有 `ShapeKind` 都能用"点列（＋可选外接框）"表达，
/// 单一路径让再生成管线只有一条代码分支，避免"每种 kind 一套几何"的爆炸。
/// `ShapeKind` 负责解释这些点的含义。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ShapeGeometry {
    /// 点列（折线点 / 手绘采样点 / 曲线锚点）
    pub anchors: Vec<ShapeAnchor>,
    /// 框型几何的外接框（可为空：纯点列形状不需要）
    pub rect: Option<ShapeRect>,
    /// 是否闭合（填充的前提；Spiderweb 的 `gaps`/闭合判定在算子层细化）
    pub closed: bool,
}

impl ShapeGeometry {
    /// 点列构造
    pub fn from_points(points: impl IntoIterator<Item = (f32, f32)>) -> Self {
        Self {
            anchors: points.into_iter().map(ShapeAnchor::new).collect(),
            rect: None,
            closed: false,
        }
    }

    /// 纯框构造
    pub fn from_rect(rect: ShapeRect) -> Self {
        Self {
            anchors: Vec::new(),
            rect: Some(rect),
            closed: true,
        }
    }

    /// 是否为空几何（既无点列也无外接框）
    pub fn is_empty(&self) -> bool {
        self.anchors.is_empty() && self.rect.is_none()
    }

    /// 几何包围盒（tick_lo, key_lo, tick_hi, key_hi）；空几何返回 `None`
    ///
    /// 用于选择框、命中测试与后续"随形状框自适应"的编辑器操作。
    pub fn bounds(&self) -> Option<(f32, f32, f32, f32)> {
        let mut acc: Option<(f32, f32, f32, f32)> = self
            .rect
            .map(|r| (r.tick_lo, r.key_lo, r.tick_hi, r.key_hi));
        for anchor in &self.anchors {
            let (tick, key) = anchor.pos;
            acc = Some(match acc {
                None => (tick, key, tick, key),
                Some((lo_t, lo_k, hi_t, hi_k)) => {
                    (lo_t.min(tick), lo_k.min(key), hi_t.max(tick), hi_k.max(key))
                }
            });
        }
        acc
    }

    /// 整体平移（tick 自由、key 保持连续值；钳制由调用方按需处理）
    pub fn translate(&mut self, dtick: f32, dkey: f32) {
        for anchor in &mut self.anchors {
            anchor.pos = (anchor.pos.0 + dtick, anchor.pos.1 + dkey);
        }
        if let Some(rect) = &mut self.rect {
            rect.tick_lo += dtick;
            rect.tick_hi += dtick;
            rect.key_lo += dkey;
            rect.key_hi += dkey;
        }
    }
}

#[cfg(test)]
mod tests;
