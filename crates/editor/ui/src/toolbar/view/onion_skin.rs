//! 洋葱皮全局显示开关按钮（工具栏右侧**单独按钮**）
//!
//! 外观为**分裂式开关**的静态形态：左侧洋葱图形（点击 = 开/关），右侧一枚
//! **下拉三角**。三角位本轮只「预留」——后续挂「洋葱皮选项」下拉（显示范围 /
//! 透明度等），届时才把它拆成第二个命中区。
//!
//! ⚠️ 为什么预留位先不拆成独立按钮：本仓历史上有过独立小三角（
//! `buttons.rs::tool_dropdown_caret`，挂在颜料桶右侧触发旧「绘制工具选择面板」），
//! 该面板迁到音符画悬浮条后函数已删除。分裂按钮一旦拆命中区就必须有真实菜单，
//! 否则那 12px 是**点了没反应**的死区。当前三角与主图标同属一个按钮：整体点击
//! 即开/关，视觉先读作分裂按钮。
//!
//! 状态反馈：`on` 用 `background.strong`（与工具选中态同源），`off` 透明，
//! 悬停用 `background.weak`。图标恒为 `icon::OnionSkin`——不做开/关两套图，
//! 关闭态由底色表达（与「音符画工具箱」开关同一口径）。

use iced_core::{Alignment, Border, Color};
use iced_widget::{button, container, row, space};

use crate::resources::icon;
use crate::toolbar::{ButtonId, Event, Toolbar};
use crate::widget;
use crate::{Element, Theme, window};
use lumino_extras::i18n::MainTranslations;

/// 主图标尺寸（与右侧其它开关按钮一致：自动滚动 / 协作为 18）
const ONION_ICON_SIZE: u32 = 18;
/// 右侧下拉三角尺寸：比主图标小一圈，读作"附属触发器"而非第二个工具
///
/// 取 12：与历史实现（`buttons.rs::tool_dropdown_caret`，颜料桶右侧小三角）
/// 同一尺寸口径——那套视觉已经被用户认过，不必再造一套比例。
const CARET_ICON_SIZE: u32 = 12;
/// 主图标与下拉三角之间的间隙（留白让两者可分辨，又不至于像两个按钮）
const CARET_GAP: f32 = 3.0;

impl Toolbar {
    /// 渲染「洋葱皮」全局显示开关
    ///
    /// 纯显示开关：只改 `Toolbar::onion_skin_enabled`（默认 `false`），
    /// 不碰当前工具 / 填充态 / 任何浮层；渲染侧读同一字段作为闸门。
    pub fn render_onion_skin_button<'a>(
        &'a self,
        content_height: f32,
        palette: &'a iced_core::theme::palette::Extended,
        t: &'static MainTranslations,
        window: &'a window::Window,
    ) -> Element<'a> {
        let is_on = self.onion_skin_enabled;

        let content = row![
            icon::view_with_size_and_theme(
                icon::OnionSkin,
                ONION_ICON_SIZE,
                ONION_ICON_SIZE,
                Some(&window.theme),
            ),
            space().width(CARET_GAP),
            // 预留的下拉三角：与主图标同一按钮，见模块头"为什么先不拆命中区"
            icon::view_with_size_and_theme(
                icon::CaretDown,
                CARET_ICON_SIZE,
                CARET_ICON_SIZE,
                Some(&window.theme),
            ),
        ]
        .align_y(Alignment::Center);

        let btn = button(content)
            .on_press(Event::toggle_onion_skin())
            .style(move |_theme: &Theme, status| {
                let bg = if is_on {
                    palette.background.strong.color
                } else if status == button::Status::Hovered {
                    palette.background.weak.color
                } else {
                    Color::TRANSPARENT
                };
                button::Style {
                    border: Border {
                        radius: 4.0.into(),
                        width: 0.0,
                        color: Color::TRANSPARENT,
                    },
                    ..Default::default()
                }
                .with_background(bg)
            })
            .padding([8, 6]);

        container(widget::with_tooltip_bottom(
            iced_widget::mouse_area(btn)
                .on_enter(Event::button_hovered(Some(ButtonId::OnionSkin)))
                .on_exit(Event::button_hovered(None)),
            t.onion_skin_tooltip,
        ))
        .height(content_height)
        .align_y(iced_core::alignment::Vertical::Center)
        .padding([0, 4])
        .style(move |_theme: &Theme| {
            container::Style::default().background(palette.background.weakest.color)
        })
        .into()
    }
}
