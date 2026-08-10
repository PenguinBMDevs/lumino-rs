//! 曲线工具直线绘制状态
//!
//! 曲线工具在钢琴卷帘上通过两次点击拉出一条直线：
//! - 第一次点击设置起点锚点；
//! - 第二次点击设置终点锚点（直线完整，显示 √ 确认 / × 取消按钮）；
//! - 锚点可独立拖动，连线整体平移；
//! - 确认后按直线经过的网格格点批量生成音符。

/// 直线工具交互阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineToolInteraction {
    /// 无交互
    #[default]
    None,
    /// 拖动起点锚点
    DraggingAnchorStart,
    /// 拖动终点锚点
    DraggingAnchorEnd,
    /// 整体平移连线
    DraggingLine,
}

/// 锚点位置（吸附后的 tick, key）
pub type LineAnchor = (f32, u16);

/// 曲线工具直线绘制状态
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LineToolState {
    /// 起点锚点；None = 未设置
    pub anchor_start: Option<LineAnchor>,
    /// 终点锚点；None = 未设置
    pub anchor_end: Option<LineAnchor>,
    /// 当前交互阶段
    pub interaction: LineToolInteraction,
    /// 拖拽基准：按下时的吸附（tick, key）
    pub drag_start_snap: LineAnchor,
    /// 拖拽基准：按下时被拖动锚点的原始值
    pub drag_anchor_orig: LineAnchor,
    /// 拖拽基准：按下时起点锚点的原始值（连线平移用）
    pub drag_line_orig_start: LineAnchor,
    /// 拖拽基准：按下时终点锚点的原始值（连线平移用）
    pub drag_line_orig_end: LineAnchor,
}

impl LineToolState {
    /// 是否已有至少一个锚点
    pub fn has_anchor(&self) -> bool {
        self.anchor_start.is_some()
    }

    /// 两个锚点是否都已设置（直线完整）
    pub fn is_complete(&self) -> bool {
        self.anchor_start.is_some() && self.anchor_end.is_some()
    }

    /// 设置下一个锚点：无锚点时设置起点，否则设置终点。
    ///
    /// 直线完整后调用不改变状态（重新开始由交互层先 `reset`）。
    pub fn set_next_anchor(&mut self, tick: f32, key: u16) {
        if self.anchor_start.is_none() {
            self.anchor_start = Some((tick, key));
        } else if self.anchor_end.is_none() {
            self.anchor_end = Some((tick, key));
        }
    }

    /// 重置整个直线状态
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_next_anchor_flow() {
        let mut state = LineToolState::default();
        assert!(!state.has_anchor());
        assert!(!state.is_complete());

        state.set_next_anchor(0.0, 60);
        assert!(state.has_anchor());
        assert_eq!(state.anchor_start, Some((0.0, 60)));
        assert!(!state.is_complete());

        state.set_next_anchor(1920.0, 64);
        assert!(state.is_complete());
        assert_eq!(state.anchor_end, Some((1920.0, 64)));
    }

    #[test]
    fn test_set_next_anchor_ignored_after_complete() {
        let mut state = LineToolState::default();
        state.anchor_start = Some((0.0, 60));
        state.anchor_end = Some((1920.0, 64));
        // 完整后 set_next_anchor 不改变状态（重新开始由交互层 reset）
        state.set_next_anchor(100.0, 30);
        assert_eq!(state.anchor_start, Some((0.0, 60)));
        assert_eq!(state.anchor_end, Some((1920.0, 64)));
    }

    #[test]
    fn test_reset_clears_all() {
        let mut state = LineToolState {
            anchor_start: Some((0.0, 60)),
            anchor_end: Some((1920.0, 64)),
            interaction: LineToolInteraction::DraggingLine,
            ..Default::default()
        };
        state.reset();
        assert_eq!(state, LineToolState::default());
    }
}
