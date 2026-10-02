//! 「关于」页 logo 彩蛋的悬浮动画层（UI-007）
//!
//! 彩蛋序列（转动 → 脱离 → 飞向随机落点 → 重力砸底 → 渐隐）期间，logo 必须脱离
//! 关于页的滚动/列布局、在整个对话框窗口内自由定位，因此由本模块渲染成一个
//! **覆盖整窗的 Stack 顶层**。
//!
//! 两条硬约束：
//! - **不捕获鼠标事件**：本层只由容器 / 占位器 / Svg 组成，不套 `mouse_area`，
//!   iced 对未捕获事件的层会向下穿透，故底层设置内容仍可正常点击；
//! - **不越界**：所有坐标由 `AboutEggState` 在视口范围内求解（落点仅取窗口中轴
//!   Y 以上、落地钳在窗口底部内），本层只做「位置 → 布局」的无损换算。
//!
//! 定位手法：iced 0.14 没有绝对定位控件，这里用「固定像素占位器」表达坐标
//! （`column![space(上边距), row![space(左边距), logo, space(Fill)], space(Fill)]`），
//! 语义直观且不依赖任何容器对齐默认值。

use iced_core::{Alignment, Length};
use iced_widget::{column, container, row, space};
use lumino_ui_core::{
    Element, Theme,
    resources::icon::{self, Icon},
    state::about_egg::{ABOUT_LOGO_BOX, ABOUT_LOGO_SIZE},
};

/// 渲染彩蛋飞行层（覆盖整个对话框窗口）
///
/// 仅在 [`lumino_ui_core::state::AboutEggState::is_airborne`] 为真时由调用方推入 Stack。
pub fn view<'a>(egg: &'a lumino_ui_core::state::AboutEggState, theme: &'a Theme) -> Element<'a> {
    let (x, y) = egg.position();

    // 旋转期尺寸补偿：抵消 iced `Svg` 按旋转后包围盒做 Contain 缩放导致的周期性缩小
    let fitted = (ABOUT_LOGO_SIZE * egg.spin_fit_scale()).round().max(1.0) as u32;
    let glyph = icon::view_transformed(
        Icon::LogoInApp,
        fitted,
        fitted,
        Some(theme),
        egg.spin_angle(),
        egg.opacity(),
    );

    // 固定方盒 + 居中：旋转时 Svg 的布局盒会长大（AABB 补偿所致），居中可以
    // 保证视觉中心恒定，且不会把外层占位器顶开。
    let slot = container(glyph)
        .width(Length::Fixed(ABOUT_LOGO_BOX))
        .height(Length::Fixed(ABOUT_LOGO_BOX))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center);

    container(column![
        space().height(y.max(0.0)),
        row![space().width(x.max(0.0)), slot, space().width(Length::Fill),],
        space().height(Length::Fill),
    ])
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
