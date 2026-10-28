//! 对话框窗口的尺寸落地（REF-001 阶段 D）
//!
//! # 时序
//!
//! ```text
//! 阶段 1  建窗（hidden，种子尺寸）+ 认领窗口归属
//! 阶段 2  建 GFX
//! 阶段 3  建 Host（viewport = 种子尺寸）+ 注入状态
//! 入册后首帧  measure_dialog_window_size → request_inner_size → 同步 viewport
//!             → 居中到主窗口 → set_visible(true) → 首次 redraw
//! ```
//!
//! # 为什么必须等到「入册后的首帧」
//!
//! 1. **事件路由**：窗口归属在阶段 1 建窗时就被认领
//!    （`DialogManager::claim_window`），因此 `request_inner_size` 造成的
//!    `Resized` 会正确回到对话框自身，而不是被误路由给主窗口。
//! 2. **内容注入时序**：找回删除音轨的条目由 Runner 在 `about_to_wait` 的
//!    `try_fill_recover_track_entries` 注入，位置在 `about_to_wait_init_dialogs`
//!    之后、`DialogManager::update()` 之前 ⇒ **只有在这里测量才能看到完整内容**。
//!    在 `initialize_ui` 末尾测量会量到空列表，窗口过小导致内容被裁剪。
//! 3. **不闪烁**：窗口在此之前一直 `with_visible(false)`，用户看不到种子尺寸。

use winit::dpi::{LogicalSize, PhysicalSize};

use lumino_ui::state::root_state::DialogType;

use super::{DialogWindow, SEED_DIALOG_SIZE};
use crate::platform::{WorkArea, centered_position};

/// 形状抽屉的初始内容只有占位，不能把 1000×700 的决策尺寸当成不可缩小的下限。
const SHAPE_DRAWER_MIN_SIZE: LogicalSize<f32> = LogicalSize::new(640.0, 480.0);

impl DialogWindow {
    /// REF-001：测量内容尺寸 → 应用窗口尺寸 → 居中 → 显示（只做一次）
    pub(crate) fn apply_measured_size(&mut self) {
        if self.sized {
            return;
        }
        self.sized = true;

        // 工作区取**主窗口所在显示器**：对话框也将被放到那块屏上。
        let work_area = self
            .parent
            .as_ref()
            .and_then(|parent| crate::platform::work_area(parent));

        let target = self.measure_target_size(work_area.as_ref());

        // E1：可缩放对话框必须在**系统层面**压不扁。
        // 下限 = 本次决策出的尺寸（内容刚好放得下的尺寸），这样用户拖小窗口时
        // Windows 会在 WM_GETMINMAXINFO 就拦住，控件不会被挤成细条/被裁掉。
        // 不可缩放的对话框无需设置（系统已经不允许拉伸）。
        if let Some(minimum) = minimum_inner_size_for(
            self.dialog_type,
            target,
            work_area.map(|area| area.logical_size),
            self.window.is_resizable(),
        ) {
            self.window.set_min_inner_size(Some(minimum));
        }

        // Windows 上 `request_inner_size` 恒返回 `None`（请求交给显示系统，
        // 真实尺寸由 `WindowEvent::Resized` 回送）——所以绝不能靠返回值判断成败，
        // 下面回读 `inner_size()` 并立即对齐渲染侧。
        let applied = self.window.request_inner_size(target);

        let physical = self.window.inner_size();
        self.sync_render_size(physical);

        self.center_on_parent(work_area.as_ref(), physical);

        // 显示：**测量失败也必须显示**（此时用种子尺寸），绝不允许窗口不可见。
        self.window.set_visible(true);
        // 显式聚焦：Windows 上首次点击未激活窗口可能只用于激活、不投递给控件，
        // 表现为“必须先点一下输入框，下拉/按钮才响应”。对话框打开即聚焦，
        // 保证第一次点击就落到目标控件。
        self.window.focus_window();

        tracing::debug!(
            "对话框尺寸已应用: {:?} 目标={:?} 同步返回={:?} 实际={:?} 工作区={:?}",
            self.dialog_type,
            target,
            applied,
            physical,
            work_area.map(|area| area.logical_size)
        );
    }

    /// 询问 UI 侧的建议尺寸；不可用时回退种子尺寸
    fn measure_target_size(&self, work_area: Option<&WorkArea>) -> LogicalSize<f32> {
        let Some(ui) = self.ui.as_ref() else {
            return SEED_DIALOG_SIZE;
        };

        let work_area_size = work_area.map_or(iced_core::Size::ZERO, |area| {
            iced_core::Size::new(area.logical_size.width, area.logical_size.height)
        });

        match usable_size(ui.measure_dialog_window_size(work_area_size)) {
            Some(size) => size,
            None => {
                tracing::warn!("对话框内容测量不可用，回退种子尺寸: {:?}", self.dialog_type);
                SEED_DIALOG_SIZE
            }
        }
    }

    /// 把渲染侧（iced viewport + swapchain surface）同步到实际物理尺寸
    ///
    /// 事件回程（`WindowEvent::Resized`）也会做同样的事，这里是**幂等**的提前量：
    /// 首帧必须按新尺寸渲染，否则会先显示一帧旧尺寸再跳变。
    fn sync_render_size(&mut self, physical: PhysicalSize<u32>) {
        if physical.width == 0 || physical.height == 0 {
            return;
        }

        if let (Some(gfx), Some(ui)) = (self.gfx.as_mut(), self.ui.as_mut()) {
            ui.resize(physical.width, physical.height);
            gfx.resize(physical.width, physical.height);
        }
    }

    /// 居中到主窗口（并夹进工作区，见 [`centered_position`]）
    fn center_on_parent(&self, work_area: Option<&WorkArea>, target: PhysicalSize<u32>) {
        let Some(parent) = self.parent.as_ref() else {
            return;
        };

        // `outer_position` 在部分平台/窗口状态下不可用；`outer_size` 在 winit 0.30
        // 直接返回尺寸（不含 Result）。
        let Ok(anchor_position) = parent.outer_position() else {
            return;
        };
        let anchor_size = parent.outer_size();

        let position = centered_position(anchor_position, anchor_size, target, work_area);
        self.window.set_outer_position(position);
    }
}

/// 可缩放对话框的**最小** inner size（纯函数，`None` = 不设下限）
///
/// # 为什么需要（E1）
///
/// 对话框构造时没有设任何最小尺寸（`with_min_inner_size` 全仓库只有主窗口用），
/// 运行时的 `set_min_inner_size` 也没有调用点 ⇒ 用户可以把可缩放对话框拖到
/// **比内容还小**，控件被挤成细条或被裁掉——尺子（`host::measure::edge`）在
/// 决策尺寸下是干净的，但管不住用户的手。这里把下限交给系统（Windows 在
/// `WM_GETMINMAXINFO` 阶段拦截），不需要新增任何尺寸数字。
///
/// 取值 = 本次决策出的尺寸，并按工作区夹取（防御：下限不得超过可见区域，
/// 否则用户连正常尺寸都拉不出来）。不可缩放 / 尺寸非法时返回 `None`。
pub(crate) fn minimum_inner_size_for(
    dialog_type: DialogType,
    decided: LogicalSize<f32>,
    work_area: Option<LogicalSize<f32>>,
    resizable: bool,
) -> Option<LogicalSize<f32>> {
    if dialog_type == DialogType::ShapeDrawer {
        return minimum_inner_size(SHAPE_DRAWER_MIN_SIZE, work_area, resizable);
    }
    minimum_inner_size(decided, work_area, resizable)
}

pub(crate) fn minimum_inner_size(
    decided: LogicalSize<f32>,
    work_area: Option<LogicalSize<f32>>,
    resizable: bool,
) -> Option<LogicalSize<f32>> {
    if !resizable {
        return None;
    }

    if !decided.width.is_finite()
        || !decided.height.is_finite()
        || decided.width <= 0.0
        || decided.height <= 0.0
    {
        return None;
    }

    let Some(work_area) = work_area else {
        return Some(decided);
    };

    if !work_area.width.is_finite()
        || !work_area.height.is_finite()
        || work_area.width <= 0.0
        || work_area.height <= 0.0
    {
        return Some(decided);
    }

    Some(LogicalSize::new(
        decided.width.min(work_area.width),
        decided.height.min(work_area.height),
    ))
}

/// 测量结果 → 可用的逻辑尺寸（纯函数，便于单测）
///
/// 拒绝 0 / 负数 / 非有限值：这些值会造出不可见或不可点击的窗口，
/// 必须由调用方回退种子尺寸。
pub(crate) fn usable_size(measured: iced_core::Size) -> Option<LogicalSize<f32>> {
    let usable = measured.width.is_finite()
        && measured.height.is_finite()
        && measured.width > 0.0
        && measured.height > 0.0;

    usable.then(|| LogicalSize::new(measured.width, measured.height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced_core::Size;

    #[test]
    fn test_usable_size_accepts_positive_finite() {
        let size = usable_size(Size::new(488.0, 180.0)).expect("正的有限尺寸应可用");
        assert_eq!(size.width, 488.0);
        assert_eq!(size.height, 180.0);
    }

    /// 「测量不可用 ⇒ 回退种子尺寸」这条回退链的入口判据：
    /// 任何一种坏值都必须返回 `None`，否则会开出 0 尺寸窗口。
    #[test]
    fn test_usable_size_rejects_degenerate_values() {
        for measured in [
            Size::new(0.0, 180.0),
            Size::new(488.0, 0.0),
            Size::new(-1.0, 180.0),
            Size::new(488.0, -1.0),
            Size::new(f32::NAN, 180.0),
            Size::new(488.0, f32::NAN),
            Size::new(f32::INFINITY, 180.0),
            Size::new(488.0, f32::INFINITY),
        ] {
            assert!(
                usable_size(measured).is_none(),
                "{measured:?} 必须判为不可用（否则会开出 0/NaN 尺寸窗口）"
            );
        }
    }

    /// 种子尺寸本身必须可用——它是测量失败时的唯一退路。
    #[test]
    fn test_seed_size_is_usable() {
        assert!(
            usable_size(Size::new(SEED_DIALOG_SIZE.width, SEED_DIALOG_SIZE.height)).is_some(),
            "种子尺寸必须非零，否则退路也会失败"
        );
    }

    const DECIDED: LogicalSize<f32> = LogicalSize::new(720.0, 540.0);
    const WORK_AREA: LogicalSize<f32> = LogicalSize::new(1920.0, 1040.0);

    /// 可缩放对话框：下限 = 决策尺寸 ⇒ 系统层面压不扁，控件不会被挤。
    #[test]
    fn test_minimum_inner_size_for_resizable_is_decided_size() {
        assert_eq!(
            minimum_inner_size(DECIDED, Some(WORK_AREA), true),
            Some(DECIDED)
        );
    }

    #[test]
    fn test_shape_drawer_min_size_is_not_pinned_to_decided_size() {
        let decided = LogicalSize::new(1000.0, 700.0);
        assert_eq!(
            minimum_inner_size_for(DialogType::ShapeDrawer, decided, Some(WORK_AREA), true),
            Some(LogicalSize::new(640.0, 480.0)),
            "形状抽屉必须允许从 1000×700 缩到 640×480"
        );
        assert_eq!(
            minimum_inner_size_for(DialogType::Settings, decided, Some(WORK_AREA), true),
            Some(decided),
            "其他可缩放对话框仍按内容尺寸设下限"
        );
    }

    /// 不可缩放对话框：系统本来就不允许拉伸，无需设下限。
    #[test]
    fn test_minimum_inner_size_is_none_for_fixed_dialogs() {
        assert_eq!(minimum_inner_size(DECIDED, Some(WORK_AREA), false), None);
    }

    /// 工作区比决策尺寸还小（异常）⇒ 下限夹到工作区，否则用户连正常尺寸都拉不出来。
    #[test]
    fn test_minimum_inner_size_clamps_to_work_area() {
        let tiny = LogicalSize::new(500.0, 400.0);

        assert_eq!(
            minimum_inner_size(DECIDED, Some(tiny), true),
            Some(LogicalSize::new(500.0, 400.0))
        );
    }

    /// 找不到工作区时退回决策尺寸（不因为拿不到显示器信息就放弃下限）。
    #[test]
    fn test_minimum_inner_size_without_work_area_uses_decided() {
        assert_eq!(minimum_inner_size(DECIDED, None, true), Some(DECIDED));
    }

    /// 决策尺寸非法 ⇒ 绝不设下限：错误的 min 可能让窗口永远显示不出来。
    #[test]
    fn test_minimum_inner_size_rejects_degenerate_decided_size() {
        for decided in [
            LogicalSize::new(0.0, 540.0),
            LogicalSize::new(720.0, 0.0),
            LogicalSize::new(-1.0, 540.0),
            LogicalSize::new(f32::NAN, 540.0),
            LogicalSize::new(720.0, f32::INFINITY),
        ] {
            assert_eq!(
                minimum_inner_size(decided, Some(WORK_AREA), true),
                None,
                "{decided:?} 必须被拒绝"
            );
        }
    }

    /// 工作区数据非法（NaN / 0）时退回决策尺寸，而不是算出 0 下限。
    #[test]
    fn test_minimum_inner_size_survives_broken_work_area() {
        for work_area in [
            LogicalSize::new(0.0, 1040.0),
            LogicalSize::new(f32::NAN, 1040.0),
            LogicalSize::new(1920.0, f32::NAN),
        ] {
            assert_eq!(
                minimum_inner_size(DECIDED, Some(work_area), true),
                Some(DECIDED),
                "工作区 {work_area:?} 非法时应退回决策尺寸"
            );
        }
    }
}
