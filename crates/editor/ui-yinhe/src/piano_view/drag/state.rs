//! 拖拽状态 — 复用 `lumino_editor_state::DragState`（ghost 方案）
//!
//! 对应 `yinhe piano_view/drag/state.rs + frame.rs + group_move.rs` 等 9 文件
//! 在 lumino 侧的 iced 桩：
//! - 选中集合与 delta 语义完全复用 `DragState`（`selected: BitVec, delta_tick: i64, delta_key: i16`）
//! - 拖动期间 `EditorData.notes` 不动，仅维护 delta；松手时一次性 `apply_to_notes`

use lumino_editor_state::DragState;

/// 钢琴卷帘拖拽交互状态（iced Program State 内持有）
///
/// 对 `yinhe drag::state::SelDragFrameState` 的 lumino 映射：
/// yinhe 用 `ui.data_mut().get_persisted` 跨帧持久化拖拽状态；
/// iced 侧由 `canvas::Program::State` 直接持有，按 Program 生命周期管理，
/// 不再依赖 egui 的 Id 持久化。
#[derive(Debug, Default)]
pub struct PianoDragState {
    /// 复用 lumino 的 ghost 拖拽状态（选中位图 + delta）
    pub drag: Option<DragState>,
    /// 是否正在框选（marquee）而非音符拖动
    pub is_marquee: bool,
    /// 框选起点本地坐标（用于绘制选框）
    pub marquee_start: Option<iced_core::Point>,
    /// 框选当前本地坐标
    pub marquee_current: Option<iced_core::Point>,
}

impl PianoDragState {
    /// 是否有进行中的拖动（音符移动或框选）
    #[must_use]
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some() || self.is_marquee
    }

    /// 是否为音符拖动（非框选）
    #[must_use]
    pub fn is_note_drag(&self) -> bool {
        self.drag.is_some()
    }

    /// 启动音符拖动
    pub fn start_note_drag(&mut self, drag: DragState) {
        self.drag = Some(drag);
        self.is_marquee = false;
    }

    /// 启动框选
    pub fn start_marquee(&mut self, start: iced_core::Point) {
        self.is_marquee = true;
        self.marquee_start = Some(start);
        self.marquee_current = Some(start);
        self.drag = None;
    }

    /// 更新框选当前位置
    pub fn update_marquee(&mut self, pos: iced_core::Point) {
        self.marquee_current = Some(pos);
    }

    /// 计算框选矩形（本地坐标，归一化）
    #[must_use]
    pub fn marquee_rect(&self) -> Option<iced_core::Rectangle> {
        let s = self.marquee_start?;
        let c = self.marquee_current?;
        let x = s.x.min(c.x);
        let y = s.y.min(c.y);
        let w = (s.x - c.x).abs();
        let h = (s.y - c.y).abs();
        if w < 3.0 || h < 3.0 {
            return None;
        }
        Some(iced_core::Rectangle::new(
            iced_core::Point::new(x, y),
            iced_core::Size::new(w, h),
        ))
    }

    /// 清空拖动状态（松手时调用）
    pub fn clear(&mut self) {
        self.drag = None;
        self.is_marquee = false;
        self.marquee_start = None;
        self.marquee_current = None;
    }
}
