//! 洋葱皮按钮（工具栏右侧**分裂式**按钮：左开/关 + 右下拉范围菜单）
//!
//! 左半（洋葱图标）点击 = 开/关洋葱皮；右半（下拉三角）点击 = 展开「显示范围」
//! 菜单（全部音轨 / 上一个 / 下一个 / 指定）。
//!
//! # 为什么现在才把三角拆成第二个命中区
//!
//! 拆分按钮的前提是**三角后面必须真有一个菜单**，否则那 12px 是点了没反应的死区。
//! 2026-11-04 首版只做「全局开关」，所以三角与主图标同属一个按钮（整体点击 = 开/关）；
//! 2026-11-05 接入范围菜单后，三角才拆成独立命中区。
//!
//! # 菜单为什么用 `iced_aw::DropDown`
//!
//! 与「音符精度」下拉（`view/status.rs`）同一套实现（`DropDown::new` 加 `on_dismiss`）。
//! 宽度、背景、圆角、菜单项悬停底色都对齐那套视觉，避免出现两种下拉手感。
//! 唯一差异是 `alignment(BottomEnd)`：洋葱皮按钮在右侧，菜单必须贴右边缘展开，
//! 居中对齐会把 190px 菜单推出窗口右边界。
//!
//! 状态反馈：`on` 用 `background.strong`（与工具选中态同源），`off` 透明，
//! 悬停用 `background.weak`。图标恒为 `icon::OnionSkin`——不做开/关两套图，
//! 关闭态由底色表达（与「音符画工具箱」开关同一口径）。

use iced_aw::DropDown;
use iced_core::{Alignment, Border, Color, Length};
use iced_widget::{button, column, container, row, space, text};

use crate::resources::icon;
use crate::toolbar::{ButtonId, Event, Toolbar};
use crate::widget;
use crate::{Element, Theme, window};
use lumino_core::tr;
use lumino_extras::i18n::MainTranslations;
use lumino_ui_core::onion_skin_mode::OnionSkinMode;

/// 主图标尺寸（与右侧其它开关按钮一致：自动滚动 / 协作为 18）
const ONION_ICON_SIZE: u32 = 18;
/// 右侧下拉三角尺寸：比主图标小一圈，读作"附属触发器"而非第二个工具
///
/// 取 12：与历史实现（`buttons.rs::tool_dropdown_caret`，颜料桶右侧小三角）
/// 同一尺寸口径——那套视觉已经被用户认过，不必再造一套比例。
const CARET_ICON_SIZE: u32 = 12;
/// 主图标与下拉三角之间的间隙（留白让两者可分辨，又不至于像两个按钮）
const CARET_GAP: f32 = 3.0;
/// 下拉菜单宽度（与音符精度下拉同为 190，两处视觉对齐）
const MENU_WIDTH: f32 = 190.0;

/// 菜单项悬停底色（与音符精度下拉逐字同款：白色 10% 叠加）
const MENU_ITEM_HOVER: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.10);

impl Toolbar {
    /// 渲染「洋葱皮」分裂按钮 + 显示范围下拉菜单
    pub fn render_onion_skin_button<'a>(
        &'a self,
        content_height: f32,
        palette: &'a iced_core::theme::palette::Extended,
        t: &'static MainTranslations,
        window: &'a window::Window,
    ) -> Element<'a> {
        let is_on = self.onion_skin_enabled;
        let text_color = palette.background.weak.text;

        // ── 左半：开/关 ──
        let icon_btn = button(icon::view_with_size_and_theme(
            icon::OnionSkin,
            ONION_ICON_SIZE,
            ONION_ICON_SIZE,
            Some(&window.theme),
        ))
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

        // ── 右半：下拉三角（独立命中区；2026-11-05 起它后面真有菜单）──
        let caret_btn = button(icon::view_with_size_and_theme(
            icon::CaretDown,
            CARET_ICON_SIZE,
            CARET_ICON_SIZE,
            Some(&window.theme),
        ))
        .on_press(Event::toggle_onion_skin_menu())
        .style(move |_theme: &Theme, status| {
            let bg = match status {
                button::Status::Hovered => palette.background.weak.color,
                _ if is_on => palette.background.strong.color,
                _ => Color::TRANSPARENT,
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
        .padding([8, 4]);

        let underlay = widget::with_tooltip_bottom(
            iced_widget::mouse_area(
                row![icon_btn, space().width(CARET_GAP), caret_btn].align_y(Alignment::Center),
            )
            .on_enter(Event::button_hovered(Some(ButtonId::OnionSkin)))
            .on_exit(Event::button_hovered(None)),
            t.onion_skin_tooltip,
        );

        // ── 菜单：四个显示范围（当前档打勾；未实现档点击后由 root 提示）──
        let mut items = column![].spacing(2);
        for mode in OnionSkinMode::all() {
            let active = mode == self.onion_skin_mode;
            let label: Element<'a> = match mode {
                OnionSkinMode::AllTracks => text(tr!("显示全部音轨")),
                OnionSkinMode::PreviousTrack => text(tr!("显示上一个音轨")),
                OnionSkinMode::NextTrack => text(tr!("显示下一个音轨")),
                OnionSkinMode::SpecificTrack => text(tr!("显示指定音轨…")),
            }
            .size(13)
            .color(text_color)
            .into();

            items = items.push(
                button(
                    row![
                        // 勾选位固定占宽，避免选中态左右跳动
                        text(if active { "✓" } else { " " })
                            .size(13)
                            .color(text_color),
                        space().width(6),
                        label,
                    ]
                    .align_y(Alignment::Center),
                )
                .on_press(Event::onion_skin_mode_selected(mode))
                .padding([4, 8])
                .width(Length::Fill)
                .style(move |_theme: &Theme, status| button::Style {
                    background: match status {
                        button::Status::Hovered => Some(MENU_ITEM_HOVER.into()),
                        _ => None,
                    },
                    text_color,
                    border: Border::default().rounded(3),
                    ..Default::default()
                }),
            );
        }

        let menu = container(items)
            .padding(6)
            .width(Length::Fixed(MENU_WIDTH))
            .style(move |_theme: &Theme| container::Style {
                background: Some(palette.background.weak.color.into()),
                text_color: Some(text_color),
                border: Border {
                    radius: 6.0.into(),
                    width: 1.0,
                    color: palette.background.strong.color,
                },
                ..Default::default()
            });

        // 贴右边缘展开：按钮在工具栏最右侧，居中对齐会把菜单推出窗口
        let dropdown = DropDown::new(underlay, menu, self.onion_skin_menu_open)
            .width(Length::Fixed(MENU_WIDTH))
            .alignment(iced_aw::drop_down::Alignment::BottomEnd)
            .on_dismiss(Event::close_onion_skin_menu());

        container(dropdown)
            .height(content_height)
            .align_y(iced_core::alignment::Vertical::Center)
            .padding([0, 4])
            .style(move |_theme: &Theme| {
                container::Style::default().background(palette.background.weakest.color)
            })
            .into()
    }
}
