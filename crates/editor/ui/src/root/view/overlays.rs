//! 覆盖层/弹窗视图渲染函数
//!
//! 包含进度窗口和对话框窗口的渲染。

use iced_core::Length;
use iced_widget::{button, checkbox, column, container, progress_bar, row, space, text};

use lumino_core::tr;

use crate::root::Root;
use crate::state::root_state::DialogType;
use crate::{Element, Theme};

/// 对话框的同尺寸底色占位。
///
/// 内容已由 egui 叠加层整窗绘制（各对话框都有对应的 `DialogModel`），iced 侧只需
/// 一块与窗口等大的背景，避免窗口透明/闪白。窗口尺寸由 `dialog_preferred_size`
/// 决定（这些对话框已钉死宽高，不依赖 iced 测量）。
fn dialog_placeholder<'a>(theme: &Theme) -> Element<'a> {
    let bg = theme.palette().background;
    container(space())
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(bg)),
            ..Default::default()
        })
        .into()
}

impl Root {
    /// 渲染进度窗口
    pub(super) fn view_progress(&self) -> Element<'_> {
        puffin::profile_scope!("root_view_progress");

        // 进度窗口只显示进度
        // 默认显示初始化状态，避免窗口空白
        let (msg, progress) = self
            .progress
            .as_ref()
            .map(|(m, p)| (m.as_str(), *p))
            .unwrap_or((tr!("正在初始化..."), 0.0));

        let content: Element<'_> = container(
            column![
                text(tr!("处理中..."))
                    .size(24)
                    .style(|theme: &Theme| text::Style {
                        color: Some(theme.extended_palette().background.neutral.text),
                    }),
                text(msg).size(16).style(|theme: &Theme| text::Style {
                    color: Some(theme.extended_palette().background.neutral.text),
                }),
                progress_bar(0.0..=1.0, progress as f32),
            ]
            .spacing(20)
            .align_x(iced_core::Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(30)
        .style(|theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(theme.palette().background)),
            ..Default::default()
        })
        .into();

        if self.use_native_titlebar {
            content
        } else {
            column![self.titlebar.view_popup(&self.window), content]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        }
    }

    /// 渲染设备检查警告窗（首启 GPU 兼容性失败提示）
    pub(super) fn view_device_warning(&self) -> Element<'_> {
        puffin::profile_scope!("root_view_device_warning");

        let suppress = self.state.device_warning_suppress_checked;

        let actions = row![
            checkbox(suppress)
                .label(tr!("不要再提示我"))
                .on_toggle(crate::window::Event::device_warning_suppress_toggled),
            space().width(Length::Fill),
            button(text(tr!("确认并关闭")).size(14)).on_press(
                crate::window::Event::device_warning_actioned(
                    crate::window::DeviceWarningAction::Quit
                ),
            ),
            button(text(tr!("放我进去")).size(14)).on_press(
                crate::window::Event::device_warning_actioned(
                    crate::window::DeviceWarningAction::Continue { suppress },
                ),
            ),
        ]
        .spacing(12)
        .align_y(iced_core::Alignment::Center);

        let content: Element<'_> = container(
            column![
                row![
                    text("🤔").size(34),
                    text(tr!("你的电脑不满足运行 Lumino 的最低要求")).size(18),
                ]
                .spacing(12)
                .align_y(iced_core::Alignment::Center),
                text(self.state.device_warning_detail.clone()).size(14),
                actions,
            ]
            .spacing(20)
            .align_x(iced_core::Alignment::Start),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(24)
        .style(|theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(theme.palette().background)),
            ..Default::default()
        })
        .into();

        if self.use_native_titlebar {
            content
        } else {
            column![self.titlebar.view_popup(&self.window), content]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        }
    }

    /// 渲染对话框
    pub(super) fn view_dialog(&self) -> Element<'_> {
        puffin::profile_scope!("root_view_dialog");

        // 对话框窗口 - 根据类型显示不同内容
        let content: Element<'_> = match self.state.dialog_type {
            // 协作：内容由 egui 整窗接管，iced 只留同尺寸底色。
            DialogType::Collaboration => dialog_placeholder(&self.window.theme),
            // 加载确认：内容由 egui 整窗接管，iced 只留同尺寸底色（420×233 钉死）。
            DialogType::LoadConfirm => dialog_placeholder(&self.window.theme),
            // 工程设置：内容由 egui 整窗接管，iced 只留同尺寸底色。
            DialogType::ProjectSettings => dialog_placeholder(&self.window.theme),
            // 设置：内容由 egui 整窗接管（chrome::settings 已覆盖所有页），
            // iced 只留同尺寸底色（720×540 钉死）。
            DialogType::Settings => dialog_placeholder(&self.window.theme),
            // 变速：内容由 egui 整窗接管，iced 只留同尺寸底色。
            DialogType::SpeedChange => dialog_placeholder(&self.window.theme),
            // 批量编辑：内容由 egui 整窗接管，iced 只留同尺寸底色。
            DialogType::BatchEdit => dialog_placeholder(&self.window.theme),
            // 自定义精度：内容由 egui 整窗接管，iced 只留同尺寸底色。
            DialogType::CustomPrecision => dialog_placeholder(&self.window.theme),
            // DialogType::None: 关闭过程中 dialog_type 可能被复位为 None，
            // 此时渲染空容器避免闪跳到精度面板。实际关闭由 DialogWindow 的 should_close 驱动。
            DialogType::None => container(space())
                .width(iced_core::Length::Fill)
                .height(iced_core::Length::Fill)
                .style(|theme: &Theme| container::Style {
                    background: Some(iced_core::Background::Color(theme.palette().background)),
                    ..Default::default()
                })
                .into(),
            // 音频导出进度：内容已由 egui 整窗接管，iced 只留同尺寸底色
            // （该对话框的尺寸是钉死的 406×233，不依赖 iced 测量）。
            DialogType::ExportProgress => dialog_placeholder(&self.window.theme),
            DialogType::VideoExport => {
                // 进度覆盖层由 egui 绘制（`DialogModel::VideoExportOverlay`）；
                // iced 只返回同尺寸底色，保证窗口测量与拖拽不变。
                let bg = self.window.theme.palette().background;
                container(space())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(move |_theme: &Theme| container::Style {
                        background: Some(iced_core::Background::Color(bg)),
                        ..Default::default()
                    })
                    .into()
            }
            // 内存监控：内容由 egui 整窗接管，iced 只留同尺寸底色。
            DialogType::MemoryMonitor => dialog_placeholder(&self.window.theme),
            // 找回删除音轨：内容由 egui 整窗接管，iced 只留同尺寸底色。
            DialogType::RecoverTrack => dialog_placeholder(&self.window.theme),
            // 保存确认：内容由 egui 整窗接管，iced 只留同尺寸底色（338×169 钉死）。
            DialogType::SaveConfirm => dialog_placeholder(&self.window.theme),
            // 画刷设置：内容由 egui 整窗接管，iced 只留同尺寸底色。
            DialogType::BrushSettings => dialog_placeholder(&self.window.theme),
            // 音符画设置：内容由 egui 整窗接管，iced 只留同尺寸底色。
            DialogType::DrawSettings => dialog_placeholder(&self.window.theme),
            // 帮助：内容由 egui 整窗接管，iced 只留同尺寸底色。
            DialogType::Help => dialog_placeholder(&self.window.theme),
            // 形状抽屉：面板 / 分割线 / 画布全部由 egui 绘制，iced 只留同尺寸底色。
            DialogType::ShapeDrawer => dialog_placeholder(&self.window.theme),
            // 撤销历史：内容由 egui 整窗接管，iced 只留同尺寸底色。
            DialogType::History => dialog_placeholder(&self.window.theme),
            // 云存储连接面板 / 云文件浏览面板（Phase 3/4 实现完整 UI）
            // 云存储三类：内容由 egui 整窗接管，iced 只留同尺寸底色。
            DialogType::CloudConnect | DialogType::CloudBrowser | DialogType::CloudNotice => {
                dialog_placeholder(&self.window.theme)
            }
        };

        let base: Element<'_> = if self.use_native_titlebar {
            content
        } else {
            column![
                self.titlebar.view_popup(&self.window),
                container(content).width(Length::Fill).height(Length::Fill),
            ]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        };

        // 关于页 logo 彩蛋（UI-007）飞行层已迁 egui（`chrome::egg`），
        // iced 只保留对话框底色；飞行层在 egui 顶层绘制且不捕获鼠标。
        base
    }
}
