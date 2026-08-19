//! 工程走带左侧音轨列表 —— 交互状态与拖拽排序辅助
//!
//! 拖拽排序交互约定：
//! - 按下音轨行 → 记录拖拽候选（`drag`），同时由 Canvas 发出
//!   `TrackReorderStarted` 注册到 Sidebar 统一计时；
//! - 移动超过阈值 → 候选激活（`active = true`），插入指示随鼠标更新；
//! - 长按超时激活由外部 `drag_active` 字段标记（Sidebar 计时）；
//! - 释放 → 取出候选，若已激活则发出 `TrackReorderEnded(insert_index)`。

use std::collections::HashSet;
use std::time::Instant;

use iced_core::{Point, keyboard};

/// 按下后移动超过该距离（像素）立即激活拖拽排序
pub const DRAG_ACTIVATE_THRESHOLD_PX: f32 = 8.0;

/// 运行时交互状态
#[derive(Debug)]
pub struct TrackListState {
    /// 运行时静音状态
    pub track_muted: Vec<bool>,
    /// 运行时独奏状态
    pub track_soloed: Vec<bool>,
    /// 多选集合
    pub selected_tracks: HashSet<usize>,
    /// 范围选择锚点（tracks 数组索引）
    pub selection_anchor: Option<usize>,
    /// 当前修饰键
    pub modifiers: keyboard::Modifiers,
    /// 上次左键点击时间
    pub last_click_time: Instant,
    /// 上次左键点击位置
    pub last_click_pos: Option<Point>,
    /// 拖拽排序候选（None = 无拖拽进行中）
    pub drag: Option<TrackDragState>,
}

impl Default for TrackListState {
    fn default() -> Self {
        Self {
            track_muted: Vec::new(),
            track_soloed: Vec::new(),
            selected_tracks: HashSet::new(),
            selection_anchor: None,
            modifiers: keyboard::Modifiers::default(),
            last_click_time: Instant::now(),
            last_click_pos: None,
            drag: None,
        }
    }
}

/// 音轨拖拽排序候选状态
#[derive(Debug, Clone)]
pub struct TrackDragState {
    /// 被拖拽的音轨 ID
    pub track_id: usize,
    /// 按下位置（相对列表顶部，含 scroll_y）
    pub press_pos: Point,
    /// 是否已激活拖拽（移动超阈值；长按激活由外部 `drag_active` 标记）
    pub active: bool,
    /// 插入位置指示（`0..=tracks.len()`，表示插入到该索引之前）
    pub hover_index: usize,
}

impl TrackListState {
    /// 记录拖拽候选（左键按下音轨行时调用）
    ///
    /// 初始插入指示为按下行下边缘（`idx + 1`），长按未移动时也直观。
    pub fn begin_drag(&mut self, track_id: usize, abs_pos: Point, idx: usize) {
        self.drag = Some(TrackDragState {
            track_id,
            press_pos: abs_pos,
            active: false,
            hover_index: idx.saturating_add(1),
        });
    }

    /// 拖拽中更新候选（鼠标移动时调用）
    ///
    /// 返回 `hover_index` 是否变化（变化时调用方应触发重绘）。
    pub fn update_drag(&mut self, abs_pos: Point, track_height: f32, len: usize) -> bool {
        let Some(drag) = self.drag.as_mut() else {
            return false;
        };
        if !drag.active {
            let dx = abs_pos.x - drag.press_pos.x;
            let dy = abs_pos.y - drag.press_pos.y;
            if dx.hypot(dy) < DRAG_ACTIVATE_THRESHOLD_PX {
                return false;
            }
            drag.active = true;
        }
        let new_idx = hover_index_at_y(abs_pos.y, track_height, len);
        if new_idx == drag.hover_index {
            return false;
        }
        drag.hover_index = new_idx;
        true
    }

    /// 结束拖拽：取出候选并清除
    pub fn take_drag(&mut self) -> Option<TrackDragState> {
        self.drag.take()
    }

    /// 拖拽是否正在生效（内部激活或外部长按激活）
    pub fn drag_effective(&self, external_active: bool) -> bool {
        self.drag
            .as_ref()
            .is_some_and(|d| d.active || external_active)
    }
}

/// 由纵向坐标（相对列表顶部，含 scroll_y）计算插入索引（`0..=len`）
pub fn hover_index_at_y(y: f32, track_height: f32, len: usize) -> usize {
    if track_height <= 0.0 {
        return 0;
    }
    let row = (y / track_height).round();
    row.clamp(0.0, len as f32) as usize
}

/// 静音/独奏按钮
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MuteSoloButton {
    /// 静音按钮
    Mute,
    /// 独奏按钮
    Solo,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with(track_count: usize) -> TrackListState {
        let mut s = TrackListState::default();
        s.track_muted.resize(track_count, false);
        s.track_soloed.resize(track_count, false);
        s
    }

    #[test]
    fn test_hover_index_at_y() {
        // 行高 48：行 i 覆盖 [i*48, (i+1)*48)，round 取最近行边界
        assert_eq!(hover_index_at_y(0.0, 48.0, 5), 0);
        assert_eq!(hover_index_at_y(24.0, 48.0, 5), 1);
        assert_eq!(hover_index_at_y(47.0, 48.0, 5), 1);
        assert_eq!(hover_index_at_y(48.0, 48.0, 5), 1);
        assert_eq!(hover_index_at_y(72.0, 48.0, 5), 2);
        assert_eq!(hover_index_at_y(240.0, 48.0, 5), 5); // 超出末尾 → len
        assert_eq!(hover_index_at_y(-10.0, 48.0, 5), 0); // 顶部之外 → 0
    }

    #[test]
    fn test_hover_index_at_y_zero_height() {
        assert_eq!(hover_index_at_y(100.0, 0.0, 5), 0);
    }

    #[test]
    fn test_begin_drag_sets_initial_hover_below_row() {
        let mut s = state_with(4);
        s.begin_drag(2, Point::new(10.0, 60.0), 1);
        let d = s.drag.as_ref().expect("候选应存在");
        assert_eq!(d.track_id, 2);
        assert!(!d.active);
        assert_eq!(d.hover_index, 2); // idx + 1
    }

    #[test]
    fn test_drag_activates_after_threshold_move() {
        let mut s = state_with(4);
        s.begin_drag(2, Point::new(10.0, 60.0), 1);

        // 微小移动不激活
        assert!(!s.update_drag(Point::new(12.0, 62.0), 48.0, 4));
        assert!(!s.drag.as_ref().expect("拖拽候选应存在").active);

        // 超过阈值激活，且 hover 更新
        assert!(s.update_drag(Point::new(30.0, 130.0), 48.0, 4));
        let d = s.drag.as_ref().expect("拖拽候选应存在");
        assert!(d.active);
        assert_eq!(d.hover_index, 3); // 130/48 ≈ 2.7 → round 3
    }

    #[test]
    fn test_drag_hover_clamped_to_len() {
        let mut s = state_with(4);
        s.begin_drag(2, Point::new(0.0, 0.0), 1);
        s.update_drag(Point::new(100.0, 1000.0), 48.0, 4);
        assert_eq!(s.drag.as_ref().expect("拖拽候选应存在").hover_index, 4);
    }

    #[test]
    fn test_take_drag_clears_candidate() {
        let mut s = state_with(4);
        s.begin_drag(2, Point::new(0.0, 0.0), 1);
        let d = s.take_drag().expect("候选应存在");
        assert_eq!(d.track_id, 2);
        assert!(s.drag.is_none());
    }

    #[test]
    fn test_drag_effective_combines_external_flag() {
        let mut s = state_with(4);
        s.begin_drag(2, Point::new(0.0, 0.0), 1);
        // 内部未激活、外部未激活 → 不生效
        assert!(!s.drag_effective(false));
        // 外部长按激活 → 生效
        assert!(s.drag_effective(true));
        // 内部移动激活 → 生效
        s.update_drag(Point::new(100.0, 100.0), 48.0, 4);
        assert!(s.drag_effective(false));
    }
}
