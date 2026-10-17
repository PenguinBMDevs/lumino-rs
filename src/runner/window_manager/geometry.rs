//! 窗口几何单位换算与 `UiState` 迁移（REF-001 阶段 G-2）
//!
//! # 背景：一个持续放大的窗口
//!
//! `UiState.w/h` 原先**写物理、读逻辑**：
//!
//! | 动作 | 位置 | 单位 |
//! |---|---|---|
//! | 写尺寸 | `WindowManager::handle_resize`（`Resized` 事件） | 物理 `PhysicalSize<u32>` |
//! | 读尺寸 | `WindowManager::build_window_attributes` | 逻辑 `LogicalSize<u32>` |
//! | 写位置 | `WindowManager::handle_event`（`Moved` 事件） | 物理 `PhysicalPosition<i32>` |
//! | 读位置 | `WindowManager::build_window_attributes` | 逻辑 `LogicalPosition<i32>` |
//!
//! 150% 缩放下：逻辑 1440×900 = 物理 2160×1350 ⇒ 关闭时存 2160×1350 ⇒
//! 下次启动按**逻辑** 2160×1350 建窗 ⇒ 物理 3240×2025 ⇒ **每次重启 ×1.5**，
//! 位置同比例向右下漂移。
//!
//! # 口径决定（本模块存在的理由）
//!
//! 统一为**逻辑像素**：
//! - 写入侧用 [`logical_size`] / [`logical_position`] 转换后落盘；
//! - 读取侧本来就按逻辑用，不动；
//! - 老文件（物理口径，无 `version` 字段）由 [`migrate_units`] 一次性换算，
//!   并夹到工作区——历史上累积的异常大值（如 3240 宽）不能被原样还原，
//!   否则用户升级后第一眼就是超出屏幕的巨窗。
//!
//! 反向选择（存物理、读物理）也能修好"越放越大"，但换显示器后窗口的**视觉大小**
//! 会跳变，且任何未来"逻辑读"代码又会踩同一个坑，故不取。

use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event_loop::ActiveEventLoop;

use lumino_core::storage::ui_state::{UI_STATE_LAYOUT_VERSION, UiState};
use lumino_ui::constants::dimensions::{MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH};

/// 缩放因子兜底
///
/// 0 / 负数 / NaN / inf 会让换算得到 inf、NaN 或反向结果（`dpi` 的
/// `to_logical` 直接做除法），因此非有限或过小的值一律按 1.0 处理。
pub(super) fn sanitize_scale(scale: f64) -> f64 {
    // 区间用 `contains`：NaN 参与比较恒为 false，因此非有限值自然落到兜底分支
    if (0.5..=8.0).contains(&scale) {
        scale
    } else {
        1.0
    }
}

/// 物理尺寸 → 逻辑尺寸（落盘口径）
pub(super) fn logical_size(physical: PhysicalSize<u32>, scale: f64) -> (u32, u32) {
    let logical = physical.to_logical::<u32>(sanitize_scale(scale));

    (logical.width, logical.height)
}

/// 物理位置 → 逻辑位置（落盘口径）
pub(super) fn logical_position(position: PhysicalPosition<i32>, scale: f64) -> (i32, i32) {
    let logical = position.to_logical::<i32>(sanitize_scale(scale));

    (logical.x, logical.y)
}

/// 状态文件是否需要一次性单位迁移（缺 `version` 字段 = 历史物理口径）
pub(crate) fn needs_migration(state: &UiState) -> bool {
    state.version < UI_STATE_LAYOUT_VERSION
}

/// 主显示器缩放因子与**逻辑**工作区（迁移的换算与夹取基准）
///
/// 取不到显示器（无头/异常环境）时按 1.0 与「不夹取」处理：
/// 1.0 下物理值等于逻辑值，迁移是恒等变换，不会损坏数据。
pub(crate) fn primary_monitor_metrics(event_loop: &ActiveEventLoop) -> (f64, (u32, u32)) {
    let Some(monitor) = event_loop.primary_monitor() else {
        return (1.0, (u32::MAX, u32::MAX));
    };

    let scale = sanitize_scale(monitor.scale_factor());
    let logical = monitor.size().to_logical::<u32>(scale);

    (scale, (logical.width, logical.height))
}

/// 把历史物理口径的 `w/h/x/y` 一次性换算成逻辑口径，并夹到工作区
///
/// 返回 `true` 表示确实做了迁移（调用方据此落盘）。
/// 幂等：`version` 已达标时直接返回 `false`，绝不重复缩小。
pub(crate) fn migrate_units(state: &mut UiState, scale: f64, work_area: (u32, u32)) -> bool {
    if !needs_migration(state) {
        return false;
    }

    let scale = sanitize_scale(scale);
    let (max_width, max_height) = work_area;

    let width = (f64::from(state.w) / scale).round() as u32;
    let height = (f64::from(state.h) / scale).round() as u32;

    state.w = width.clamp(MIN_WINDOW_WIDTH, max_width.max(MIN_WINDOW_WIDTH));
    state.h = height.clamp(MIN_WINDOW_HEIGHT, max_height.max(MIN_WINDOW_HEIGHT));
    state.x = state.x.map(|x| {
        (f64::from(x) / scale)
            .round()
            .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
    });
    state.y = state.y.map(|y| {
        (f64::from(y) / scale)
            .round()
            .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
    });
    state.version = UI_STATE_LAYOUT_VERSION;

    true
}

#[cfg(test)]
mod tests {
    //! REF-001 G-2 的红线：单位口径的往返、幂等、跨 DPI 与老文件迁移。
    //!
    //! 这些用例不碰 winit 窗口（本文件全是纯函数），`cargo test -p lumino-rs` 就能跑。

    use super::*;

    fn legacy(w: u32, h: u32, x: Option<i32>, y: Option<i32>) -> UiState {
        UiState {
            x,
            y,
            w,
            h,
            is_maximized: false,
            version: 0,
        }
    }

    /// 150% 下：物理 2160×1350 → 落盘逻辑 1440×900
    #[test]
    fn persist_uses_logical_pixels_at_150_percent() {
        let (w, h) = logical_size(PhysicalSize::new(2160, 1350), 1.5);
        assert_eq!((w, h), (1440, 900), "物理 2160×1350 @150% 的落盘逻辑尺寸");
    }

    /// 同尺寸在同 DPI 下反复落盘 → 值不变（不会二次缩小）
    #[test]
    fn persist_is_idempotent() {
        let first = logical_size(PhysicalSize::new(1000, 700), 1.5);

        // 还原：逻辑 667×467 在 150% 下是物理 1000.5×700.5 → 1001×701
        let restored = PhysicalSize::new(
            (f64::from(first.0) * 1.5).round() as u32,
            (f64::from(first.1) * 1.5).round() as u32,
        );
        let second = logical_size(restored, 1.5);

        assert_eq!(first, second, "落盘值必须收敛，不能每次少一像素");
        assert_eq!(first, (667, 467));
    }

    /// 跨 DPI：150% 落盘的逻辑值，在 100% 下还原出的**逻辑**尺寸不变
    #[test]
    fn logical_size_survives_dpi_change() {
        let persisted = logical_size(PhysicalSize::new(2160, 1350), 1.5);

        // 100% 启动：读取侧按逻辑用 ⇒ 逻辑尺寸就是落盘值，与 DPI 无关
        assert_eq!(persisted, (1440, 900));
    }

    /// 位置同样换算（旧值物理 → 逻辑）
    #[test]
    fn position_is_converted_to_logical() {
        assert_eq!(
            logical_position(PhysicalPosition::new(300, 450), 1.5),
            (200, 300)
        );
        assert_eq!(
            logical_position(PhysicalPosition::new(-300, -450), 1.5),
            (-200, -300)
        );
    }

    /// 老文件（无 version）迁移：物理 → 逻辑 + 版本号落定
    #[test]
    fn legacy_state_is_migrated_once() {
        let mut state = legacy(2160, 1350, Some(300), Some(450));

        assert!(migrate_units(&mut state, 1.5, (2560, 1400)));
        assert_eq!((state.w, state.h), (1440, 900));
        assert_eq!((state.x, state.y), (Some(200), Some(300)));
        assert_eq!(state.version, UI_STATE_LAYOUT_VERSION);

        // 第二次迁移必须是空操作（幂等，绝不重复缩小）
        assert!(!migrate_units(&mut state, 1.5, (2560, 1400)));
        assert_eq!((state.w, state.h), (1440, 900));
    }

    /// 100% 屏上的老文件：物理即逻辑，迁移恒等（老用户无感）
    #[test]
    fn legacy_state_on_100_percent_is_unchanged() {
        let mut state = legacy(2160, 1350, Some(300), Some(450));

        assert!(migrate_units(&mut state, 1.0, (3840, 2160)));
        assert_eq!((state.w, state.h), (2160, 1350));
        assert_eq!((state.x, state.y), (Some(300), Some(450)));
    }

    /// 历史 bug 累积的异常大值必须被夹到工作区，并抬到最小窗口尺寸
    #[test]
    fn migrated_state_is_clamped_to_work_area_and_minimum() {
        let mut state = legacy(4860, 300, None, None);

        assert!(migrate_units(&mut state, 1.5, (2560, 1400)));
        assert_eq!(state.w, 2560, "迁移结果超出工作区宽度必须被夹住");
        assert_eq!(state.h, MIN_WINDOW_HEIGHT, "低于最小高度必须抬到下限");
    }

    /// 非法缩放因子兜底为 1.0（否则物理/0 会得到 inf，`as u32` 变成 0）
    #[test]
    fn invalid_scale_falls_back_to_one() {
        assert_eq!(sanitize_scale(0.0), 1.0);
        assert_eq!(sanitize_scale(f64::NAN), 1.0);
        assert_eq!(sanitize_scale(f64::INFINITY), 1.0);
        assert_eq!(sanitize_scale(-1.5), 1.0);
        assert_eq!(sanitize_scale(1.5), 1.5);

        assert_eq!(logical_size(PhysicalSize::new(1440, 900), 0.0), (1440, 900));
    }
}
