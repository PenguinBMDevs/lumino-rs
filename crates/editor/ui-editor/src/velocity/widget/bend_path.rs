//! 弯音贝塞尔路径编辑状态（本地编辑，支持实时生效 / √× 确认两种模式）
//!
//! 参考卷帘曲线工具（`interaction/line_tool`）的贝塞尔路径语义：
//! 锚点带 in/out 控制柄（自动柄 = 段方向 1/3 = 精确直线），
//! 用户拖动控制柄后标记自定义，段按实际柄弯曲。
//!
//! - 实时生效模式：交互操作即时同步到 `AutomationLane`（发 `AutomationEdit`）；
//! - √× 确认模式：操作只修改本地 `anchors`，√ 确认（`BendPathConfirm`）
//!   后全量重建 lane，× 取消（`BendPathCancel`）丢弃。
//!
//! 坐标语义：逻辑坐标 (tick, value)，tick 为连续值（绘制中不吸附、
//! 落定时吸附网格），value 为连续值（落定时取整）。

use lumino_note_core::automation::{AutomationEvent, SegmentShape};

/// 弯音路径锚点（逻辑坐标 + 贝塞尔控制柄）
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BendAnchor {
    /// 锚点位置 (tick, value)
    pub pos: (f32, f32),
    /// 出向控制柄偏移（相对 pos）
    pub out_handle: (f32, f32),
    /// 入向控制柄偏移（相对 pos）
    pub in_handle: (f32, f32),
    /// 控制柄是否自动维护（未被用户自定义）
    pub handles_auto: bool,
}

impl BendAnchor {
    /// 构造锚点（控制柄自动维护，偏移为 0——由重算填充）
    pub fn new(pos: (f32, f32)) -> Self {
        Self {
            pos,
            out_handle: (0.0, 0.0),
            in_handle: (0.0, 0.0),
            handles_auto: true,
        }
    }

    /// 设置出向控制柄（标记为自定义）。
    ///
    /// 钳制：出向柄 tick 偏移不允许 < 0（不能越过锚点垂直切线），
    /// 防止曲线回环导致同一 tick 多个弯音值。
    pub fn set_out_handle(&mut self, offset: (f32, f32)) {
        self.out_handle = (offset.0.max(0.0), offset.1);
        self.handles_auto = false;
    }

    /// 设置入向控制柄（标记为自定义）。
    ///
    /// 钳制：入向柄 tick 偏移不允许 > 0（不能越过锚点垂直切线）。
    pub fn set_in_handle(&mut self, offset: (f32, f32)) {
        self.in_handle = (offset.0.min(0.0), offset.1);
        self.handles_auto = false;
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

    /// 转换为自动化事件（√ 确认写入 lane 用）。
    ///
    /// 锚点语义 = `Curve{tension:0}` 段（贝塞尔路径），控制柄按原样携带；
    /// 自动柄事件在 `apply_automation_edit` 重算后为直线。
    pub fn to_event(&self) -> AutomationEvent {
        let mut evt = AutomationEvent::new(
            self.pos.0.round().max(0.0) as u32,
            self.pos.1.round().clamp(0.0, 16383.0) as u16,
            SegmentShape::Curve { tension: 0 },
        );
        if !self.handles_auto {
            evt.set_out_handle(self.out_handle);
            evt.set_in_handle(self.in_handle);
        }
        evt
    }
}

/// 弯音路径交互阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BendInteraction {
    /// 无交互
    #[default]
    None,
    /// 拖动锚点
    DraggingAnchor { idx: usize },
    /// 拖动控制柄
    DraggingHandle { idx: usize, side: HandleSide },
}

/// 控制柄方位
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleSide {
    /// 入向控制柄（控制"来自上一锚点"的段）
    In,
    /// 出向控制柄（控制"到下一锚点"的段）
    Out,
}

/// 弯音路径编辑状态（Canvas 状态的一部分，跨帧保留）
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BendPathState {
    /// 路径锚点（每次点击追加一个锚点，立即生效）
    pub anchors: Vec<BendAnchor>,
    /// 当前交互阶段
    pub interaction: BendInteraction,
    /// 当前选中的锚点索引（点击锚点选中，点击空白追加新锚点并选中它）
    pub selected: Option<usize>,
    /// 拖拽基准：按下时被拖锚点的原始值
    pub drag_anchor_orig: BendAnchor,
    /// 拖拽基准：按下时被拖控制柄的原始偏移
    pub drag_handle_orig: (f32, f32),
    /// 按下锚点时的屏幕位置（点击 vs 拖动判定：移动距离低于阈值视为
    /// 纯点击选中，不改变锚点高度）
    pub drag_press_screen: Option<(f32, f32)>,
}

impl BendPathState {
    /// 是否已有至少一个锚点
    pub fn has_anchor(&self) -> bool {
        !self.anchors.is_empty()
    }

    /// 是否存在完整路径（>= 2 个锚点）
    pub fn is_complete(&self) -> bool {
        self.anchors.len() >= 2
    }

    /// 是否处于任何拖拽/绘制交互中
    pub fn is_interacting(&self) -> bool {
        self.interaction != BendInteraction::None
    }

    /// 重算全部自动控制柄：相邻锚点间的柄取段方向 1/3 长度（直线条件）。
    /// 仅重算 `handles_auto` 的柄，用户自定义柄保持原值。
    pub fn recompute_auto_handles(&mut self) {
        for i in 0..self.anchors.len().saturating_sub(1) {
            let a = self.anchors[i];
            let b = self.anchors[i + 1];
            if a.handles_auto {
                self.anchors[i].out_handle = ((b.pos.0 - a.pos.0) / 3.0, (b.pos.1 - a.pos.1) / 3.0);
            }
            if b.handles_auto {
                self.anchors[i + 1].in_handle =
                    ((a.pos.0 - b.pos.0) / 3.0, (a.pos.1 - b.pos.1) / 3.0);
            }
        }
    }

    /// 清空路径（× 取消 / 模式切换）
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// 锚点可见的控制柄（首锚点只显示 out、尾锚点只显示 in、中间 in+out）
    pub fn visible_handle_sides(&self, index: usize) -> Vec<HandleSide> {
        if index == 0 {
            vec![HandleSide::Out]
        } else if index + 1 >= self.anchors.len() {
            vec![HandleSide::In]
        } else {
            vec![HandleSide::In, HandleSide::Out]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_anchor_auto_handle() {
        let a = BendAnchor::new((960.0, 8192.0));
        assert!(a.handles_auto);
        assert_eq!(a.out_handle, (0.0, 0.0));
    }

    #[test]
    fn test_set_handle_marks_custom() {
        let mut a = BendAnchor::new((0.0, 0.0));
        a.set_out_handle((320.0, 500.0));
        assert!(!a.handles_auto);
        assert_eq!(a.out_handle_abs(), (320.0, 500.0));
    }

    #[test]
    fn test_set_handle_clamps_loopback() {
        // 出向柄 tick 偏移不允许 < 0（越过锚点垂直切线 = 曲线回环）
        let mut a = BendAnchor::new((0.0, 8192.0));
        a.set_out_handle((-500.0, 3000.0));
        assert_eq!(a.out_handle.0, 0.0, "出向柄被钳制在垂直切线");
        assert_eq!(a.out_handle.1, 3000.0, "value 偏移不受限");
        // 入向柄 tick 偏移不允许 > 0
        let mut b = BendAnchor::new((960.0, 8192.0));
        b.set_in_handle((500.0, -3000.0));
        assert_eq!(b.in_handle.0, 0.0, "入向柄被钳制在垂直切线");
        assert_eq!(b.in_handle.1, -3000.0);
    }

    #[test]
    fn test_recompute_auto_handles_line() {
        let mut path = BendPathState {
            anchors: vec![
                BendAnchor::new((0.0, 8192.0)),
                BendAnchor::new((960.0, 10000.0)),
            ],
            ..Default::default()
        };
        path.recompute_auto_handles();
        assert_eq!(path.anchors[0].out_handle, (320.0, 602.6667));
        assert_eq!(path.anchors[1].in_handle, (-320.0, -602.6667));
        // 自定义柄不被覆盖
        path.anchors[1].set_in_handle((-100.0, -200.0));
        path.recompute_auto_handles();
        assert_eq!(path.anchors[1].in_handle, (-100.0, -200.0));
        assert_eq!(path.anchors[0].out_handle, (320.0, 602.6667));
    }

    #[test]
    fn test_visible_handle_sides() {
        let path = BendPathState {
            anchors: vec![
                BendAnchor::new((0.0, 0.0)),
                BendAnchor::new((960.0, 0.0)),
                BendAnchor::new((1920.0, 0.0)),
            ],
            ..Default::default()
        };
        assert_eq!(path.visible_handle_sides(0), vec![HandleSide::Out]);
        assert_eq!(
            path.visible_handle_sides(1),
            vec![HandleSide::In, HandleSide::Out]
        );
        assert_eq!(path.visible_handle_sides(2), vec![HandleSide::In]);
    }

    #[test]
    fn test_to_event_auto_handle() {
        let a = BendAnchor::new((960.5, 8192.4));
        let evt = a.to_event();
        assert_eq!(evt.tick, 961);
        assert_eq!(evt.value, 8192);
        assert!(evt.handles_auto);
    }

    #[test]
    fn test_to_event_custom_handle() {
        let mut a = BendAnchor::new((0.0, 8192.0));
        a.set_out_handle((320.0, 500.0));
        let evt = a.to_event();
        assert!(!evt.handles_auto);
        assert_eq!(evt.out_handle, (320.0, 500.0));
    }

    #[test]
    fn test_to_event_clamps_value() {
        let a = BendAnchor::new((0.0, -100.0));
        assert_eq!(a.to_event().value, 0);
        let b = BendAnchor::new((0.0, 20000.0));
        assert_eq!(b.to_event().value, 16383);
    }
}
