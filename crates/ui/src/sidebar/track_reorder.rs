//! 音轨拖拽排序 —— 状态与排序逻辑
//!
//! 侧边栏音轨选项卡与工程走带左侧音轨列表共用同一套拖拽状态机：
//! - 按下 → `candidate`（长按计时中，由 Host 每帧 `AnimationTick` 驱动计时）；
//! - 长按超时或移动超阈值 → `active`（显示插入位置指示）；
//! - 释放 → 执行排序并清除。
//!
//! `Sidebar.tracks` 是音轨顺序的唯一权威源：排序后侧边栏、工程走带
//! 与 WGPU 渲染端（`track_order`）均从该数组重建，自动保持同步。

use std::time::{Duration, Instant};

use super::core::Sidebar;

/// 长按激活拖拽的时长（毫秒）
pub const TRACK_REORDER_LONG_PRESS_MS: u64 = 350;
/// 按下后移动超过该距离（像素）立即激活拖拽
pub const TRACK_REORDER_DRAG_THRESHOLD_PX: f32 = 8.0;
/// 侧边栏音轨行高（像素，用于由鼠标坐标推算插入索引）
pub const TRACK_ROW_HEIGHT: f32 = 34.0;
/// 音轨列表首行起点偏移（面板内边距 + 标题行，像素）
pub const TRACK_ROW_OFFSET_Y: f32 = 28.0;

/// 音轨拖拽排序状态（侧边栏选项卡 + 工程走带共用计时）
///
/// 生命周期：按下 → `candidate`（长按计时中）→ 长按超时或移动超阈值 → `active`
/// → 释放 → 执行排序并清除。
#[derive(Debug, Clone)]
pub struct TrackReorderState {
    /// 被拖拽的音轨 ID
    pub track_id: usize,
    /// 按下时间（长按计时起点）
    pub started_at: Instant,
    /// 按下位置（列表局部坐标，用于移动激活阈值判断）
    pub press_pos: iced_core::Point,
    /// 是否已激活拖拽（长按超时或移动超过阈值）
    pub active: bool,
    /// 当前插入位置指示（音轨间分割线位置，`0..=tracks.len()`）
    pub hover_index: Option<usize>,
}

impl TrackReorderState {
    /// 由鼠标局部坐标更新插入位置指示（侧边栏行高推算）
    pub fn set_hover_from_y(&mut self, y: f32, track_count: usize) {
        let row = ((y - TRACK_ROW_OFFSET_Y) / TRACK_ROW_HEIGHT).round();
        let idx = row.clamp(0.0, track_count as f32) as usize;
        self.hover_index = Some(idx);
    }
}

impl Sidebar {
    /// 是否有音轨拖拽候选进行中（供 Host 每帧驱动长按计时）
    pub fn track_reorder_pending(&self) -> bool {
        self.track_reorder.is_some()
    }

    /// 处理拖拽候选开始（左键按下音轨行）
    ///
    /// 覆盖式设置候选：先前未完成的拖拽候选被丢弃（等价于取消）。
    pub fn handle_track_reorder_started(&mut self, track_id: usize, pos: iced_core::Point) {
        let from = self.tracks.iter().position(|t| t.id == track_id);
        self.track_reorder = Some(TrackReorderState {
            track_id,
            started_at: Instant::now(),
            press_pos: pos,
            active: false,
            // 初始指示：按下行下边缘（未移动时也直观）
            hover_index: from.map(|i| i + 1),
        });
    }

    /// 处理拖拽中鼠标移动（列表局部坐标）
    ///
    /// 移动超过阈值立即激活拖拽；激活后持续更新插入位置指示。
    /// 侧边栏的 `on_press` 无法提供按下坐标（传 `(0,0)`），首次移动事件
    /// 用于校准按下位置，避免"点击即激活"导致指示线闪烁。
    pub fn handle_track_reorder_moved(&mut self, pos: iced_core::Point) {
        let Some(state) = self.track_reorder.as_mut() else {
            return;
        };
        if !state.active {
            if state.press_pos == iced_core::Point::new(0.0, 0.0) {
                // 侧边栏场景：首个移动事件校准按下位置（走带 Canvas 传真实坐标，不受影响）
                state.press_pos = pos;
                return;
            }
            let dx = pos.x - state.press_pos.x;
            let dy = pos.y - state.press_pos.y;
            if dx.hypot(dy) < TRACK_REORDER_DRAG_THRESHOLD_PX {
                return;
            }
            state.active = true;
        }
        state.set_hover_from_y(pos.y, self.tracks.len());
        // Conductor 首位不变量：插入指示不允许出现在 conductor 之前
        if let Some(ci) = self.tracks.iter().position(|t| t.is_conductor)
            && let Some(hover) = state.hover_index.as_mut()
            && *hover <= ci
        {
            *hover = ci + 1;
        }
    }

    /// 长按计时（每帧由 AnimationTick 驱动）：超过阈值后激活拖拽
    pub fn update_track_reorder_timer(&mut self, now: Instant) {
        let Some(state) = self.track_reorder.as_mut() else {
            return;
        };
        if !state.active
            && now.duration_since(state.started_at)
                >= Duration::from_millis(TRACK_REORDER_LONG_PRESS_MS)
        {
            state.active = true;
        }
    }

    /// 处理拖拽排序结束（释放）
    ///
    /// `insert_index` 为 `None` 时使用内部 `hover_index`（侧边栏场景）。
    /// 仅在拖拽已激活且目标位置有效时执行排序。
    pub fn handle_track_reorder_ended(&mut self, insert_index: Option<usize>) {
        let Some(state) = self.track_reorder.take() else {
            return;
        };
        if !state.active {
            return; // 未激活 = 普通点击，仅选中（已由按下时处理）
        }
        let target = insert_index.or(state.hover_index);
        if let Some(target) = target {
            self.reorder_track(state.track_id, target);
        }
    }

    /// 将指定音轨移动到目标插入位置（`insert_index` 为 `0..=len`，表示插入到该索引之前）
    ///
    /// Conductor 首位不变量：主控音轨（`is_conductor`）必须保持在第一位——
    /// - 主控音轨自身不可被移动；
    /// - 其他音轨的目标插入位置自动钳制到主控音轨之后，不允许插入到它前面。
    pub fn reorder_track(&mut self, track_id: usize, insert_index: usize) {
        let Some(from) = self.tracks.iter().position(|t| t.id == track_id) else {
            return;
        };
        // Conductor 不可移动
        if self.tracks[from].is_conductor {
            return;
        }
        // 其他音轨不允许插入到 conductor 之前（conductor 缺席时无限制）
        let conductor_idx = self.tracks.iter().position(|t| t.is_conductor);
        let mut target = match conductor_idx {
            Some(ci) => insert_index.max(ci + 1),
            None => insert_index,
        };
        // 向后移动时，移除自身后目标位置减一
        if from < target {
            target -= 1;
        }
        let target = target.min(self.tracks.len().saturating_sub(1));
        if target == from {
            return;
        }
        let track = self.tracks.remove(from);
        self.tracks.insert(target, track);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidebar::core::Track;

    /// 构造仅含指定 id 音轨的 Sidebar（默认含 Conductor/Setup 两轨）
    fn sidebar_with_ids(ids: &[usize]) -> Sidebar {
        let mut s = Sidebar::new();
        s.tracks = ids
            .iter()
            .map(|id| Track {
                id: *id,
                name: format!("Track{}", id),
                port: 0,
                channel: 0,
                display_label: "A01".to_string(),
                is_conductor: *id == 0,
                can_delete: *id != 0,
                is_muted: false,
                is_soloed: false,
                color: None,
            })
            .collect();
        s.selected_track = ids.first().copied().unwrap_or(0);
        s
    }

    fn ids(s: &Sidebar) -> Vec<usize> {
        s.tracks.iter().map(|t| t.id).collect()
    }

    #[test]
    fn test_reorder_track_to_top() {
        // 拖到顶部：Conductor 首位不变量 → 钳制到 conductor 之后（索引 1）
        let mut s = sidebar_with_ids(&[0, 1, 2, 3]);
        s.reorder_track(3, 0);
        assert_eq!(ids(&s), vec![0, 3, 1, 2]);
    }

    #[test]
    fn test_reorder_track_to_bottom() {
        let mut s = sidebar_with_ids(&[0, 1, 2, 3]);
        s.reorder_track(1, 4);
        assert_eq!(ids(&s), vec![0, 2, 3, 1]);
    }

    #[test]
    fn test_reorder_track_to_middle() {
        let mut s = sidebar_with_ids(&[0, 1, 2, 3]);
        s.reorder_track(1, 3);
        assert_eq!(ids(&s), vec![0, 2, 1, 3]);
    }

    #[test]
    fn test_reorder_track_downward_insert_before_adjacent() {
        let mut s = sidebar_with_ids(&[0, 1, 2, 3]);
        s.reorder_track(2, 4);
        assert_eq!(ids(&s), vec![0, 1, 3, 2]);
    }

    #[test]
    fn test_reorder_track_noop_when_target_is_self() {
        let mut s = sidebar_with_ids(&[0, 1, 2, 3]);
        s.reorder_track(1, 1); // 原地
        s.reorder_track(1, 2); // 相邻下方 = 原地
        assert_eq!(ids(&s), vec![0, 1, 2, 3]);
        // 末尾音轨移到末尾 = 原地
        s.reorder_track(3, 4);
        assert_eq!(ids(&s), vec![0, 1, 2, 3]);
    }

    #[test]
    fn test_conductor_cannot_be_moved() {
        let mut s = sidebar_with_ids(&[0, 1, 2, 3]);
        s.reorder_track(0, 4); // 拖到底部
        s.reorder_track(0, 0); // 拖到顶部
        s.reorder_track(0, 2); // 拖到中间
        assert_eq!(ids(&s), vec![0, 1, 2, 3], "conductor 必须保持在第一位");
    }

    #[test]
    fn test_reorder_guard_when_conductor_not_first() {
        // 防御场景：conductor 因历史状态不在首位时，其他音轨也不允许插到它前面
        let mut s = sidebar_with_ids(&[1, 0, 2]); // conductor(id=0) 在索引 1
        // 视觉顶部音轨（id=1）尝试移到最前 → 钳制到 conductor 之后
        s.reorder_track(1, 0);
        assert_eq!(ids(&s), vec![0, 1, 2]);
    }

    #[test]
    fn test_hover_clamped_to_after_conductor() {
        let mut s = sidebar_with_ids(&[0, 1, 2, 3]);
        s.handle_track_reorder_started(2, iced_core::Point::new(0.0, 0.0));
        s.handle_track_reorder_moved(iced_core::Point::new(50.0, 100.0)); // 校准
        // 拖到列表顶部（conductor 上方）→ hover 钳制到 1
        s.handle_track_reorder_moved(iced_core::Point::new(60.0, 2.0));
        assert_eq!(s.track_reorder.as_ref().unwrap().hover_index, Some(1));
    }

    #[test]
    fn test_reorder_track_unknown_id_is_noop() {
        let mut s = sidebar_with_ids(&[0, 1, 2]);
        s.reorder_track(99, 0);
        assert_eq!(ids(&s), vec![0, 1, 2]);
    }

    #[test]
    fn test_reorder_track_preserves_track_fields() {
        let mut s = sidebar_with_ids(&[0, 1, 2, 3]);
        s.tracks[1].is_muted = true;
        s.reorder_track(1, 3);
        assert_eq!(ids(&s), vec![0, 2, 1, 3]);
        assert!(s.tracks[2].is_muted, "音轨整体移动，状态应跟随");
    }

    #[test]
    fn test_reorder_ended_without_activation_does_not_sort() {
        let mut s = sidebar_with_ids(&[0, 1, 2, 3]);
        s.handle_track_reorder_started(2, iced_core::Point::new(0.0, 0.0));
        assert!(s.track_reorder_pending());
        // 未激活直接结束 → 不排序
        s.handle_track_reorder_ended(Some(0));
        assert!(!s.track_reorder_pending());
        assert_eq!(ids(&s), vec![0, 1, 2, 3]);
    }

    #[test]
    fn test_reorder_ended_with_activation_sorts() {
        let mut s = sidebar_with_ids(&[0, 1, 2, 3]);
        s.handle_track_reorder_started(2, iced_core::Point::new(0.0, 0.0));
        // 首个移动事件校准按下位置（侧边栏 on_press 无坐标），随后移动激活
        s.handle_track_reorder_moved(iced_core::Point::new(100.0, 40.0));
        s.handle_track_reorder_moved(iced_core::Point::new(150.0, 5.0)); // 拖到列表顶部
        assert!(s.track_reorder.as_ref().unwrap().active);
        // Conductor 首位保护：顶部 hover 钳制到 1
        assert_eq!(s.track_reorder.as_ref().unwrap().hover_index, Some(1));
        s.handle_track_reorder_ended(None); // 使用内部 hover_index
        assert_eq!(ids(&s), vec![0, 2, 1, 3]);
    }

    #[test]
    fn test_reorder_timer_activates_after_long_press() {
        let mut s = sidebar_with_ids(&[0, 1, 2, 3]);
        s.handle_track_reorder_started(2, iced_core::Point::new(0.0, 0.0));
        // 未超时：不激活
        s.update_track_reorder_timer(Instant::now());
        assert!(!s.track_reorder.as_ref().unwrap().active);
        // 模拟 500ms 前的按下
        s.track_reorder.as_mut().unwrap().started_at = Instant::now() - Duration::from_millis(500);
        s.update_track_reorder_timer(Instant::now());
        assert!(s.track_reorder.as_ref().unwrap().active);
    }

    #[test]
    fn test_reorder_moved_calibrates_press_pos_first() {
        let mut s = sidebar_with_ids(&[0, 1, 2, 3]);
        s.handle_track_reorder_started(1, iced_core::Point::new(0.0, 0.0));
        // 首次移动仅校准：不激活（防止点击闪烁指示线）
        s.handle_track_reorder_moved(iced_core::Point::new(3.0, 3.0));
        let state = s.track_reorder.as_ref().unwrap();
        assert!(!state.active);
        assert_eq!(state.press_pos, iced_core::Point::new(3.0, 3.0));
        // 校准后微小移动仍不激活
        s.handle_track_reorder_moved(iced_core::Point::new(4.0, 4.0));
        assert!(!s.track_reorder.as_ref().unwrap().active);
        // 超过阈值（距校准点 > 8px）激活
        s.handle_track_reorder_moved(iced_core::Point::new(20.0, 4.0));
        assert!(s.track_reorder.as_ref().unwrap().active);
    }

    #[test]
    fn test_reorder_moved_updates_hover() {
        let mut s = sidebar_with_ids(&[0, 1, 2, 3]);
        s.handle_track_reorder_started(1, iced_core::Point::new(0.0, 0.0));
        // 微小移动不激活也不更新 hover
        s.handle_track_reorder_moved(iced_core::Point::new(2.0, 2.0));
        assert!(!s.track_reorder.as_ref().unwrap().active);
        // 超阈值移动激活并更新 hover（行高 34，起点 28）
        s.handle_track_reorder_moved(iced_core::Point::new(50.0, 100.0));
        let state = s.track_reorder.as_ref().unwrap();
        assert!(state.active);
        // (100-28)/34 ≈ 2.12 → round 2
        assert_eq!(state.hover_index, Some(2));
    }

    #[test]
    fn test_set_hover_from_y_clamps_bounds() {
        let mut state = TrackReorderState {
            track_id: 1,
            started_at: Instant::now(),
            press_pos: iced_core::Point::new(0.0, 0.0),
            active: true,
            hover_index: None,
        };
        // 顶部之外 → 0
        state.set_hover_from_y(-50.0, 4);
        assert_eq!(state.hover_index, Some(0));
        // 底部之外 → len
        state.set_hover_from_y(500.0, 4);
        assert_eq!(state.hover_index, Some(4));
        // 行边界附近 → 最近索引
        state.set_hover_from_y(28.0 + 34.0 * 2.0, 4);
        assert_eq!(state.hover_index, Some(2));
    }
}
