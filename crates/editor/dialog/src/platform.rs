//! 平台相关能力
//!
//! REF-001 阶段 E：对话框的尺寸上限与初始位置都依赖**显示器工作区**
//! （剔除任务栏/Dock 后的可见区域）。winit 0.30 不暴露工作区 API
//! （`MonitorHandle` 只有整屏 `size()`），Windows 走 `GetMonitorInfoW().rcWork`，
//! 其它平台退化为整屏尺寸（已知降级：不排除任务栏，见 [`work_area`]）。

#[cfg(target_os = "windows")]
pub mod windows;

use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::window::Window;

/// 窗口所在显示器的工作区
#[derive(Debug, Clone, Copy)]
pub struct WorkArea {
    /// 工作区左上角（物理像素，虚拟屏坐标；副屏可为负）
    pub physical_position: PhysicalPosition<i32>,
    /// 工作区物理尺寸
    pub physical_size: PhysicalSize<u32>,
    /// 工作区逻辑尺寸（供 iced 布局/测量使用）
    pub logical_size: LogicalSize<f32>,
}

/// 取窗口所在显示器的工作区
///
/// - Windows：`MonitorFromWindow(MONITOR_DEFAULTTONEAREST)` + `rcWork`，
///   与窗口的 DPI 归属一致（PerMonitorV2）。
/// - 其它平台：退化为整屏 `MonitorHandle::size()`——**不剔除任务栏/Dock**，
///   相当于把上限放宽一点，不会导致窗口不可见。
///
/// 返回 `None` 表示无法确定工作区：调用方必须退化为「只受声明上限约束」，
/// **绝不允许因此不显示窗口**。
pub fn work_area(window: &Window) -> Option<WorkArea> {
    #[cfg(target_os = "windows")]
    {
        let rect = windows::monitor_work_rect(window)?;
        let scale_factor = window.scale_factor() as f32;

        Some(WorkArea {
            physical_position: windows::work_rect_position(rect),
            physical_size: windows::work_rect_physical_size(rect)?,
            logical_size: windows::work_rect_size(rect, scale_factor)?,
        })
    }

    #[cfg(not(target_os = "windows"))]
    {
        let monitor = window.current_monitor()?;
        let scale_factor = monitor.scale_factor() as f32;

        if !scale_factor.is_finite() || scale_factor <= 0.0 {
            return None;
        }

        let physical_size = monitor.size();
        if physical_size.width == 0 || physical_size.height == 0 {
            return None;
        }

        Some(WorkArea {
            physical_position: monitor.position(),
            physical_size,
            logical_size: LogicalSize::new(
                physical_size.width as f32 / scale_factor,
                physical_size.height as f32 / scale_factor,
            ),
        })
    }
}

/// 把 `target_size` 的窗口居中到锚点窗口，并夹进工作区（返回物理坐标）
///
/// 用**物理**像素做算术：锚点与目标各自可能是不同的显示器 DPI，
/// 逻辑坐标相减会引入无法归因的误差。居中后再夹取，保证：
/// 窗口不会跑出工作区；窗口比工作区还大时贴工作区左上角（而不是被推到负坐标）。
pub fn centered_position(
    anchor_position: PhysicalPosition<i32>,
    anchor_size: PhysicalSize<u32>,
    target_size: PhysicalSize<u32>,
    work_area: Option<&WorkArea>,
) -> PhysicalPosition<i32> {
    let centered_x = anchor_position.x + (anchor_size.width as i32 - target_size.width as i32) / 2;
    let centered_y =
        anchor_position.y + (anchor_size.height as i32 - target_size.height as i32) / 2;
    let mut position = PhysicalPosition::new(centered_x, centered_y);

    if let Some(work) = work_area {
        let min_x = work.physical_position.x;
        let min_y = work.physical_position.y;
        let max_x = min_x + work.physical_size.width as i32 - target_size.width as i32;
        let max_y = min_y + work.physical_size.height as i32 - target_size.height as i32;

        // `max(..).max(min)`：窗口比工作区大时 `max_x < min_x`，此时贴左上角而不是 panic。
        position.x = position.x.clamp(min_x, max_x.max(min_x));
        position.y = position.y.clamp(min_y, max_y.max(min_y));
    }

    position
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work_area_at(x: i32, y: i32, width: u32, height: u32) -> WorkArea {
        WorkArea {
            physical_position: PhysicalPosition::new(x, y),
            physical_size: PhysicalSize::new(width, height),
            logical_size: LogicalSize::new(width as f32, height as f32),
        }
    }

    /// 常规情形：在主窗口（1920×1040 工作区内的 1200×800）中心居中。
    #[test]
    fn test_centered_position_centers_on_anchor() {
        let work = work_area_at(0, 0, 1920, 1040);
        let position = centered_position(
            PhysicalPosition::new(100, 100),
            PhysicalSize::new(1200, 800),
            PhysicalSize::new(500, 300),
            Some(&work),
        );

        assert_eq!(position.x, 100 + (1200 - 500) / 2);
        assert_eq!(position.y, 100 + (800 - 300) / 2);
    }

    /// 锚点贴住屏幕右下角时，居中结果必须被夹回工作区（不越界）。
    #[test]
    fn test_centered_position_clamps_into_work_area() {
        let work = work_area_at(0, 0, 1920, 1040);
        let position = centered_position(
            PhysicalPosition::new(1800, 1000),
            PhysicalSize::new(1200, 800),
            PhysicalSize::new(500, 300),
            Some(&work),
        );

        assert_eq!(position.x, 1920 - 500);
        assert_eq!(position.y, 1040 - 300);
    }

    /// 左侧副屏（负坐标）同样要被夹进那块屏的工作区。
    ///
    /// 构造：副屏工作区 `x ∈ [-1920, 0]`，锚点是贴该屏左上角的小窗口
    /// （-1920, 0, 400×300），目标窗口 500×300 比锚点宽 ⇒ 居中结果
    /// `x = -1920 - 50 = -1970` 会越过副屏左边界，必须被夹回 `-1920`。
    #[test]
    fn test_centered_position_handles_negative_secondary_screen() {
        let work = work_area_at(-1920, 0, 1920, 1040);
        let position = centered_position(
            PhysicalPosition::new(-1920, 0),
            PhysicalSize::new(400, 300),
            PhysicalSize::new(500, 300),
            Some(&work),
        );

        assert_eq!(position.x, -1920, "不得越过副屏左边界");
        assert_eq!(position.y, 0, "不得越过副屏上边界");
    }

    /// 窗口比工作区还大：贴左上角（而不是产生负坐标或 panic）。
    #[test]
    fn test_centered_position_oversized_target_pins_to_origin() {
        let work = work_area_at(0, 0, 800, 600);
        let position = centered_position(
            PhysicalPosition::new(0, 0),
            PhysicalSize::new(800, 600),
            PhysicalSize::new(1200, 900),
            Some(&work),
        );

        assert_eq!(position.x, 0);
        assert_eq!(position.y, 0);
    }

    /// 工作区不可用时只做居中，不做夹取（`None` 不得 panic）。
    #[test]
    fn test_centered_position_without_work_area() {
        let position = centered_position(
            PhysicalPosition::new(200, 100),
            PhysicalSize::new(1000, 700),
            PhysicalSize::new(400, 300),
            None,
        );

        assert_eq!(position.x, 200 + (1000 - 400) / 2);
        assert_eq!(position.y, 100 + (700 - 300) / 2);
    }
}
