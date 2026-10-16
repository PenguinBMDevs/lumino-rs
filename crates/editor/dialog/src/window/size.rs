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

use super::{DialogWindow, SEED_DIALOG_SIZE};
use crate::platform::{WorkArea, centered_position};

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

        // Windows 上 `request_inner_size` 恒返回 `None`（请求交给显示系统，
        // 真实尺寸由 `WindowEvent::Resized` 回送）——所以绝不能靠返回值判断成败，
        // 下面回读 `inner_size()` 并立即对齐渲染侧。
        let applied = self.window.request_inner_size(target);

        let physical = self.window.inner_size();
        self.sync_render_size(physical);

        self.center_on_parent(work_area.as_ref(), physical);

        // 显示：**测量失败也必须显示**（此时用种子尺寸），绝不允许窗口不可见。
        self.window.set_visible(true);

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
}
