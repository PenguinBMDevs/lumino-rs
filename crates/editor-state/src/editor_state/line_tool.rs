//! 曲线工具贝塞尔路径绘制状态
//!
//! 曲线工具在钢琴卷帘上通过点击拉出一条路径（初始为直线）：
//! - 前两次点击设置首尾端点（tick 按网格吸附、key 为整数格）；
//! - 点击线段中间可插入锚点（**不吸附网格**，自由精确定位）；
//! - 每段为三次贝塞尔曲线，锚点带 in/out 两个控制柄（首尾各显示一个），
//!   拖动控制柄弯曲曲线；
//! - 端点拖动保持吸附，中间锚点自由移动；
//! - 双击中间锚点删除（端点不可删）；
//! - 确认后按曲线经过的网格格点批量生成音符。

/// 直线工具交互阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineToolInteraction {
    /// 无交互
    #[default]
    None,
    /// 拖动指定锚点
    DraggingAnchor(usize),
    /// 整体平移整条路径（segment = 按下时命中的曲线段索引，
    /// 用于未拖动（视为点击插入锚点）时定位插入位置）
    DraggingLine { segment: usize },
    /// 拖动控制柄
    DraggingHandle { anchor_idx: usize, side: HandleSide },
}

/// 控制柄方位
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleSide {
    /// 入向控制柄（控制"来自上一锚点"的贝塞尔段）
    In,
    /// 出向控制柄（控制"到下一锚点"的贝塞尔段）
    Out,
}

/// 贝塞尔锚点
///
/// 位置与控制柄均为 (tick, key) 逻辑坐标；key 为 f32——
/// 中间锚点不吸附网格，可自由精确定位。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BezierAnchor {
    /// 锚点位置（tick, key）
    pub pos: (f32, f32),
    /// 出向控制柄偏移（相对 pos，控制"到下一锚点"的贝塞尔段）
    pub out_handle: (f32, f32),
    /// 入向控制柄偏移（相对 pos，控制"来自上一锚点"的贝塞尔段）
    pub in_handle: (f32, f32),
}

impl BezierAnchor {
    /// 直线退化构造：控制柄与锚点重合（贝塞尔退化为直线段）
    pub fn new(pos: (f32, f32)) -> Self {
        Self {
            pos,
            out_handle: (0.0, 0.0),
            in_handle: (0.0, 0.0),
        }
    }
    /// 出向控制柄绝对坐标（逻辑坐标）
    pub fn out_handle_abs(&self) -> (f32, f32) {
        (
            self.pos.0 + self.out_handle.0,
            self.pos.1 + self.out_handle.1,
        )
    }

    /// 入向控制柄绝对坐标（逻辑坐标）
    pub fn in_handle_abs(&self) -> (f32, f32) {
        (self.pos.0 + self.in_handle.0, self.pos.1 + self.in_handle.1)
    }
}

/// 曲线工具贝塞尔路径状态
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LineToolState {
    /// 锚点路径（有序，>= 2 个为完整状态）
    pub anchors: Vec<BezierAnchor>,
    /// 当前交互阶段
    pub interaction: LineToolInteraction,
    /// 拖拽基准：按下时的吸附（tick, key）——端点锚点/整条平移的增量基准
    pub drag_start_snap: (f32, f32),
    /// 拖拽基准：按下时的原始（tick, key）——中间锚点/控制柄的增量基准
    pub drag_start_raw: (f32, f32),
    /// 拖拽基准：按下时被拖锚点的原始值
    pub drag_anchor_orig: BezierAnchor,
    /// 拖拽基准：平移时整条路径的原始值
    pub drag_line_orig: Vec<BezierAnchor>,
    /// 拖拽基准：按下时被拖控制柄的原始偏移
    pub drag_handle_orig: (f32, f32),
    /// 按下待定标志：曲线段按下后移动超过阈值才确认拖动；
    /// 未确认松开视为点击（插入锚点）
    pub drag_confirmed: bool,
}

impl LineToolState {
    /// 是否已有至少一个锚点
    pub fn has_anchor(&self) -> bool {
        !self.anchors.is_empty()
    }

    /// 路径是否完整（>= 2 个锚点）
    pub fn is_complete(&self) -> bool {
        self.anchors.len() >= 2
    }

    /// 追加锚点（未完整时设置端点用）
    pub fn push_anchor(&mut self, pos: (f32, f32)) {
        self.anchors.push(BezierAnchor::new(pos));
    }

    /// 在段 [index-1, index] 之间插入锚点（index ∈ 1..=len），
    /// 位置为点击处（不吸附网格）。越界返回 false。
    pub fn insert_anchor_at(&mut self, index: usize, pos: (f32, f32)) -> bool {
        if index == 0 || index > self.anchors.len() {
            return false;
        }
        self.anchors.insert(index, BezierAnchor::new(pos));
        true
    }

    /// 删除指定锚点；仅中间锚点可删（端点不可删，保留至少 2 个锚点）。
    pub fn delete_anchor(&mut self, index: usize) -> bool {
        if index == 0 || index + 1 >= self.anchors.len() {
            return false;
        }
        self.anchors.remove(index);
        true
    }

    /// 指定锚点可见的控制柄（首锚点只显示 out、尾锚点只显示 in、
    /// 中间锚点显示 in + out 两个）
    pub fn visible_handle_sides(&self, index: usize) -> Vec<HandleSide> {
        if index == 0 {
            vec![HandleSide::Out]
        } else if index + 1 >= self.anchors.len() {
            vec![HandleSide::In]
        } else {
            vec![HandleSide::In, HandleSide::Out]
        }
    }

    /// 重置整个路径状态
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anchor_chain_flow() {
        let mut state = LineToolState::default();
        assert!(!state.has_anchor());
        assert!(!state.is_complete());

        state.push_anchor((0.0, 60.0));
        assert!(state.has_anchor());
        assert_eq!(state.anchors.len(), 1);
        assert!(!state.is_complete());

        state.push_anchor((1920.0, 64.0));
        assert!(state.is_complete());
        assert_eq!(state.anchors.len(), 2);
    }

    #[test]
    fn test_insert_anchor_at_segment() {
        let mut state = LineToolState::default();
        state.push_anchor((0.0, 60.0));
        state.push_anchor((1920.0, 64.0));
        // 在段 0（锚点 0→1）之间插入：插入到索引 1
        assert!(state.insert_anchor_at(1, (960.0, 62.5)));
        assert_eq!(state.anchors.len(), 3);
        assert_eq!(state.anchors[1].pos, (960.0, 62.5));
        // 新锚点控制柄与锚点重合（直线退化）
        assert_eq!(state.anchors[1].out_handle, (0.0, 0.0));
        assert_eq!(state.anchors[1].in_handle, (0.0, 0.0));
    }

    #[test]
    fn test_insert_anchor_rejects_invalid_index() {
        let mut state = LineToolState::default();
        state.push_anchor((0.0, 60.0));
        state.push_anchor((1920.0, 64.0));
        assert!(!state.insert_anchor_at(0, (1.0, 1.0)), "索引 0 非法");
        assert!(!state.insert_anchor_at(3, (1.0, 1.0)), "索引越界非法");
        assert_eq!(state.anchors.len(), 2);
    }

    #[test]
    fn test_delete_anchor_only_middle() {
        let mut state = LineToolState::default();
        state.push_anchor((0.0, 60.0));
        state.push_anchor((960.0, 62.0));
        state.push_anchor((1920.0, 64.0));

        // 端点不可删
        assert!(!state.delete_anchor(0));
        assert!(!state.delete_anchor(2));
        assert_eq!(state.anchors.len(), 3);

        // 中间锚点可删
        assert!(state.delete_anchor(1));
        assert_eq!(state.anchors.len(), 2);
        assert_eq!(state.anchors[0].pos, (0.0, 60.0));
        assert_eq!(state.anchors[1].pos, (1920.0, 64.0));
    }

    #[test]
    fn test_handle_abs_positions() {
        let mut anchor = BezierAnchor::new((100.0, 50.0));
        anchor.out_handle = (30.0, -10.0);
        anchor.in_handle = (-20.0, 5.0);
        assert_eq!(anchor.out_handle_abs(), (130.0, 40.0));
        assert_eq!(anchor.in_handle_abs(), (80.0, 55.0));
    }

    #[test]
    fn test_reset_clears_all() {
        let mut state = LineToolState::default();
        state.push_anchor((0.0, 60.0));
        state.push_anchor((1920.0, 64.0));
        state.interaction = LineToolInteraction::DraggingLine { segment: 0 };
        state.drag_confirmed = true;
        state.reset();
        assert_eq!(state, LineToolState::default());
    }
}
