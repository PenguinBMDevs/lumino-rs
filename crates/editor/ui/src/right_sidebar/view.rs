//! 右侧栏视图渲染

use iced_core::{Alignment, Color, Length};
use iced_widget::{Column, Row, Space, button, container, mouse_area, tooltip};
use lumino_extras::i18n::{Language, main_translations};
use lumino_message::{I2mConfigField, RightSidebarAction};

use crate::resources::icon::{self, Icon};
use crate::right_sidebar::core::{
    PALETTE_ALGORITHMS, RESIZE_HANDLE_WIDTH, ROUTE_BAR_WIDTH, RightSidebar, RightSidebarPanel,
};
use crate::widget;
use crate::{Element, Message, Theme, window};

/// 渲染右侧栏视图（图标按钮列 + 向左展开的面板）
pub fn view<'a>(
    right_sidebar: &'a RightSidebar,
    window: &'a window::Window,
    language: Language,
) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let t = main_translations(language);

    // 图标列（垂直排列按钮）
    let col = Column::new()
        .spacing(2)
        .width(ROUTE_BAR_WIDTH)
        .height(Length::Fill)
        // 图片转 MIDI 按钮：始终可见，点击自动展开/收起面板并亮灯
        .push(sidebar_button(
            Icon::ImageToMidi,
            t.tool_image_to_midi,
            Message::RightSidebar(RightSidebarAction::ImageToMidiClicked),
            right_sidebar.is_panel_active(RightSidebarPanel::ImageToMidi),
            window,
        ))
        // 素材库按钮：点击切换到素材库面板并亮灯
        .push(sidebar_button(
            Icon::MaterialLibrary,
            t.material_library,
            Message::RightSidebar(RightSidebarAction::MaterialLibraryClicked),
            right_sidebar.is_panel_active(RightSidebarPanel::Materials),
            window,
        ));
    let col = col.push(sidebar_button(
        Icon::PianoWaterfall,
        t.piano_waterfall,
        Message::RightSidebar(RightSidebarAction::PianoWaterfallClicked),
        right_sidebar.is_panel_active(RightSidebarPanel::PianoWaterfall),
        window,
    ));
    let col = col.push(Space::new().height(Length::Fill));

    // 图标列容器
    let route_bar = container(col)
        .width(ROUTE_BAR_WIDTH)
        .height(Length::Fill)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style::default().background(palette.background.weaker.color)
        });

    // 如果面板可见，渲染内容面板 + 调整手柄
    if right_sidebar.panel_visible {
        // 面板内容按路由分发：素材库面板 / 图片转 MIDI 面板
        let panel_content: Element<'a> = match right_sidebar.active_panel {
            RightSidebarPanel::Materials => {
                crate::right_sidebar::materials_view::panel(right_sidebar, language, window)
            }
            RightSidebarPanel::PianoWaterfall => {
                crate::right_sidebar::piano_waterfall::panel(right_sidebar, language, window)
            }
            RightSidebarPanel::ImageToMidi => i2m_panel(right_sidebar, window, language),
        };

        // 调整大小手柄（放在面板左侧边缘，紧贴主内容区——面板向右栏图标列方向
        // 展开，左侧边界才是用户肉眼可见的可拖拽边缘）
        let resize_handle = mouse_area(
            container(
                Space::new()
                    .width(Length::Fixed(RESIZE_HANDLE_WIDTH))
                    .height(Length::Fill),
            )
            .style(move |_theme: &Theme| {
                let bg = if right_sidebar.is_resizing {
                    palette.primary.strong.color
                } else {
                    palette.background.weakest.color
                };
                container::Style::default().background(bg)
            }),
        )
        .interaction(iced_core::mouse::Interaction::ResizingHorizontally)
        .on_press(Message::RightSidebar(RightSidebarAction::ResizeDragStarted))
        .on_release(Message::RightSidebar(RightSidebarAction::ResizeDragEnded));

        // 调整手柄 + 面板内容（手柄在面板左侧）
        let panel_with_handle = Row::new().push(resize_handle).push(panel_content);
        let panel_container = container(panel_with_handle)
            .width(Length::Fixed(right_sidebar.panel_width))
            .height(Length::Fill);

        // 顺序：面板内容（向左展开）→ 图标列（固定在右侧）
        Row::new()
            .push(panel_container)
            .push(route_bar)
            .height(Length::Fill)
            .into()
    } else {
        // 面板不可见，只显示图标列
        route_bar.into()
    }
}

/// 图片转 MIDI 面板内容（原 view 主体）
fn i2m_panel<'a>(
    right_sidebar: &'a RightSidebar,
    window: &'a window::Window,
    _language: Language,
) -> Element<'a> {
    // 面板内"选择图片文件"按钮：标准 iced 按钮（无图标），居左放置
    let select_btn = button(iced_widget::text("选择图片文件").size(13))
        .padding(6)
        .style(move |theme: &Theme, status| {
            let p = theme.extended_palette();
            let bg = match status {
                button::Status::Hovered | button::Status::Pressed => p.background.base.color,
                _ => p.background.weak.color,
            };
            button::Style {
                text_color: p.background.base.text,
                border: iced_core::Border {
                    radius: 4.0.into(),
                    width: 0.0,
                    color: Color::TRANSPARENT,
                },
                ..Default::default()
            }
            .with_background(bg)
        })
        .on_press(Message::RightSidebar(RightSidebarAction::SelectImageFile));

    // 文件选择按钮居左，选中文件后在按钮右侧提示
    let mut select_row = Row::new().spacing(6).align_y(Alignment::Center);
    select_row = select_row.push(select_btn);
    if right_sidebar.selected_image_path.is_some() {
        select_row = select_row.push(iced_widget::text("选择了一个图片文件").size(12).style(
            |theme: &Theme| iced_widget::text::Style {
                color: Some(theme.extended_palette().background.strong.text),
            },
        ));
    }

    // 转换参数配置区（始终显示，便于预设参数）
    let config_section = build_config_section(right_sidebar);

    // 转换按钮：仅在选中文件后出现（标准 iced 按钮，无图标）
    let mut content_col = Column::new()
        .spacing(8)
        .padding(8)
        .width(Length::Fill)
        .push(panel_header("图片转 MIDI", window))
        .push(select_row)
        .push(config_section);
    if right_sidebar.selected_image_path.is_some() {
        let convert_btn = button(
            iced_widget::text(if right_sidebar.converting {
                "转换中..."
            } else {
                "转换为 MIDI"
            })
            .size(13),
        )
        .width(Length::Fill)
        .padding(6)
        .style(move |theme: &Theme, status| {
            let p = theme.extended_palette();
            let disabled = right_sidebar.converting;
            let bg = match status {
                button::Status::Hovered | button::Status::Pressed if !disabled => {
                    p.primary.base.color
                }
                _ => p.background.weak.color,
            };
            let text_color = if disabled {
                p.background.strong.text
            } else {
                p.background.base.text
            };
            button::Style {
                text_color,
                border: iced_core::Border {
                    radius: 4.0.into(),
                    width: 0.0,
                    color: Color::TRANSPARENT,
                },
                ..Default::default()
            }
            .with_background(bg)
        })
        .on_press(Message::RightSidebar(RightSidebarAction::ConvertClicked));
        content_col = content_col.push(convert_btn);
    }

    let content = container(
        // 面板内容可能超出可视高度（参数区），包滚动容器
        iced_widget::scrollable(content_col).height(Length::Fill),
    )
    .width(Length::Fixed(
        right_sidebar.panel_width - RESIZE_HANDLE_WIDTH,
    ))
    .height(Length::Fill)
    .style(|theme: &Theme| {
        let palette = theme.extended_palette();
        container::Style::default().background(palette.background.weakest.color)
    });

    content.into()
}

/// 面板标题文本（跟随主题：暗色白、亮色黑）
fn panel_header<'a>(title: &'a str, _window: &'a window::Window) -> Element<'a> {
    iced_widget::text(title)
        .size(14)
        .style(|theme: &Theme| iced_widget::text::Style {
            color: Some(theme.extended_palette().background.neutral.text),
        })
        .into()
}

/// 转换参数配置区：key 范围、目标高度、每像素 tick、颜色数、调色板算法
fn build_config_section<'a>(right_sidebar: &'a RightSidebar) -> Element<'a> {
    let cfg = &right_sidebar.config;

    // 调色板算法下拉（选项为中文名，选中后回传索引）
    let palette_names: Vec<&'static str> =
        PALETTE_ALGORITHMS.iter().map(|(name, _)| *name).collect();
    let palette_current = palette_names
        .get(cfg.palette_index)
        .copied()
        .unwrap_or(palette_names[0]);
    let palette_control = iced_widget::pick_list(
        palette_names,
        Some(palette_current),
        |name: &'static str| {
            let idx = PALETTE_ALGORITHMS
                .iter()
                .position(|(n, _)| *n == name)
                .unwrap_or(0);
            Message::RightSidebar(RightSidebarAction::I2mPaletteChanged(idx))
        },
    )
    .text_size(12)
    .padding([3, 6])
    .width(Length::Fixed(118.0));

    Column::new()
        .spacing(4)
        .push(section_label("转换参数"))
        .push(config_row(
            "Key 范围",
            Row::new()
                .push(config_input(&cfg.start_key_text, I2mConfigField::StartKey))
                .push(iced_widget::text("~").size(12).style(|theme: &Theme| {
                    iced_widget::text::Style {
                        color: Some(theme.extended_palette().background.strong.text),
                    }
                }))
                .push(config_input(&cfg.end_key_text, I2mConfigField::EndKey))
                .spacing(4)
                .align_y(Alignment::Center)
                .into(),
        ))
        .push(config_row(
            "目标高度",
            config_input(&cfg.target_height_text, I2mConfigField::TargetHeight),
        ))
        .push(config_row(
            "每像素 tick",
            config_input(&cfg.ticks_per_pixel_text, I2mConfigField::TicksPerPixel),
        ))
        .push(config_row(
            "颜色数",
            config_input(&cfg.color_count_text, I2mConfigField::ColorCount),
        ))
        .push(config_row("调色板", palette_control.into()))
        .into()
}

/// 小节标题（跟随主题：暗色白、亮色黑，与项目内面板标题一致）
fn section_label<'a>(title: &'a str) -> Element<'a> {
    iced_widget::text(title)
        .size(12)
        .style(|theme: &Theme| iced_widget::text::Style {
            color: Some(theme.extended_palette().background.neutral.text),
        })
        .into()
}

/// 单行配置：标签居左（文字色跟随主题），控件居右
fn config_row<'a>(label: &'a str, control: Element<'a>) -> Element<'a> {
    Row::new()
        .push(
            iced_widget::text(label)
                .size(12)
                .style(|theme: &Theme| iced_widget::text::Style {
                    color: Some(theme.extended_palette().background.strong.text),
                }),
        )
        .push(Space::new().width(Length::Fill))
        .push(control)
        .spacing(4)
        .align_y(Alignment::Center)
        .into()
}

/// 小型数字输入框（带边框，仅接受数字）
fn config_input<'a>(value: &'a str, field: I2mConfigField) -> Element<'a> {
    container(
        iced_widget::text_input("", value)
            .on_input(move |text| {
                Message::RightSidebar(RightSidebarAction::I2mConfigTextChanged { field, text })
            })
            .padding([3, 6])
            .size(iced_core::Pixels(12.0))
            .width(Length::Fixed(42.0)),
    )
    .style(|theme: &Theme| {
        let palette = theme.extended_palette();
        container::Style {
            background: Some(palette.background.weak.color.into()),
            border: iced_core::Border {
                radius: 4.0.into(),
                width: 1.0,
                color: palette.background.strong.color,
            },
            ..Default::default()
        }
    })
    .into()
}

/// 与左侧栏统一的按钮样式：48x48，右侧2px指示条（激活时亮灯），图标+间距12px
fn sidebar_button<'a>(
    icon_enum: Icon,
    tooltip_text: &'a str,
    on_press: Message,
    active: bool,
    window: &'a window::Window,
) -> Element<'a> {
    let palette = window.theme.extended_palette();
    // 右侧2px指示条：激活时亮灯（与左侧栏同理，但位置在右侧），否则透明
    let split =
        container(Space::new())
            .width(2)
            .height(Length::Fill)
            .style(move |_theme: &Theme| {
                let background = if active {
                    palette.primary.base.color
                } else {
                    Color::TRANSPARENT
                };
                container::Style::default().background(background)
            });

    let icon_img = icon::view_with_size_and_theme(icon_enum, 20, 20, Some(&window.theme));

    // 图标容器占满除去右侧指示条外的宽度，使图标在按钮内水平居中
    let icon_holder = container(icon_img)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill);

    // 镜像布局：图标居中，指示条固定在右侧
    let inner = Row::new()
        .push(icon_holder)
        .push(split)
        .width(Length::Fill)
        .height(Length::Fill);

    let btn = button(inner)
        .width(48)
        .height(48)
        .padding(0)
        .style(move |theme: &Theme, status| {
            use button::Status::*;
            let p = theme.extended_palette();
            let text_color = match status {
                Hovered | Pressed => p.background.base.color,
                _ => p.background.weakest.color,
            };
            button::Style {
                text_color,
                ..Default::default()
            }
            .with_background(Color::TRANSPARENT)
        })
        .on_press(on_press);

    widget::with_tooltip(btn, tooltip_text, tooltip::Position::Left).into()
}
