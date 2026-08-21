//! 音频/视频导出面板与瀑布流全屏播放器

use iced_core::{Length, Size};
use iced_widget::{column, container, responsive, row, scrollable, text};

use crate::root::Root;
use crate::view::audio_export_dialog::view_audio_export_dialog;
use crate::view::video_export_dialog::view_video_export_dialog;
use crate::{Element, Theme};

impl Root {
    /// 渲染音频渲染面板（在主界面钢琴卷帘区域显示）
    pub(crate) fn view_audio_export_panel(&self) -> Element<'_> {
        puffin::profile_scope!("root_view_audio_export_panel");

        let theme = &self.window.theme;
        let palette = theme.extended_palette();

        container(
            container(scrollable(view_audio_export_dialog(
                &self.state.audio_export_dialog,
                theme,
            )))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_theme: &iced_core::Theme| container::Style {
                background: Some(iced_core::Background::Color(palette.background.base.color)),
                ..Default::default()
            }),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    /// 渲染视频渲染面板（在主界面钢琴卷帘区域显示）
    /// 导出进度+预览已移至独立 VideoExport 对话框窗口
    pub(crate) fn view_video_export_panel(&self) -> Element<'_> {
        puffin::profile_scope!("root_view_video_export_panel");

        let theme = &self.window.theme;
        let palette = theme.extended_palette();

        container(
            container(scrollable(view_video_export_dialog(
                &self.state.video_export_dialog,
                theme,
            )))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_theme: &iced_core::Theme| container::Style {
                background: Some(iced_core::Background::Color(palette.background.base.color)),
                ..Default::default()
            }),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    /// 渲染器入口面板（视频剪辑窗口，首级面板）
    ///
    /// 对应 nezha 的中央预览区：当 `GroupId::Renderer` 激活且未进入子面板时，
    /// 左侧 220px 预留轨道列表位置，中央预览 16:9 黑底容器内通过 shader 直合成离屏纹理，
    /// 尺寸通过 `responsive` 写回 `waterfall_player.size` 供 Host 离屏定尺寸（无拉伸）。
    pub(crate) fn view_renderer_panel(&self) -> Element<'_> {
        puffin::profile_scope!("root_view_renderer_panel");

        let waterfall_view = self.waterfall_player.view.clone();
        let size_cell = &self.waterfall_player.size;
        let clip_zoom = self.state.video_clip.zoom;
        let export_state = self.state.video_export_dialog.clone();
        let theme = self.window.theme.clone();
        let total_ticks = self.editor.editor_state.view.total_ticks;
        let ppq = self.editor.editor_state.view.ppq;
        let tempos: Vec<(u32, f32)> = self
            .editor
            .editor_state
            .data
            .tempo_points
            .iter()
            .map(|tp| (tp.tick as u32, tp.bpm as f32))
            .collect();

        responsive(move |size: Size| {
            let palette = theme.extended_palette();
            // 提取 Copy 的颜色，避免闭包返回 Element 时借用 palette 悬空
            let weak_text = palette.background.weak.text;
            let weakest_color = palette.background.weakest.color;
            let strong_text = palette.background.strong.text;
            let strong_color = palette.background.strong.color;
            let base_color = palette.background.base.color;
            let neutral_text = palette.background.neutral.text;

            // 16:9 预览尺寸：单一事实源（UI 布局与离屏纹理存储共用，比例必然一致）
            let (preview_w, preview_h) =
                crate::view::video_clip::layout::renderer_panel_preview_size(size);
            size_cell
                .borrow_mut()
                .replace((preview_w as u32, preview_h as u32));

            // 左侧轨道占位（后续接入真实轨道列表）
            let left_panel = container(
                iced_widget::column![
                    iced_widget::text("轨道").size(12).style(move |_t: &iced_core::Theme| {
                        iced_widget::text::Style {
                            color: Some(weak_text)
                        }
                    }),
                    iced_widget::space().height(8),
                    iced_widget::text("（轨道列表占位）")
                        .size(11)
                        .style(move |_t: &iced_core::Theme| iced_widget::text::Style {
                            color: Some(strong_text)
                        }),
                ]
                .padding(12),
            )
            .width(Length::Fixed(crate::view::video_clip::layout::LEFT_RESERVED))
            .height(Length::Fill)
            .style(move |_t: &iced_core::Theme| container::Style {
                background: Some(weakest_color.into()),
                border: iced_core::Border {
                    color: strong_color,
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..Default::default()
            });

            // 预览内容：有纹理则 shader 直合成，否则占位文本（提取 owned 颜色，避免借用 theme）
            let preview_content: crate::Element<'_> =
                if let Some(view) = waterfall_view.clone() {
                    // 复用右侧栏同款 WaterfallPrimitive shader
                    struct PreviewProgram {
                        view: std::sync::Arc<iced_wgpu::wgpu::TextureView>,
                    }
                    impl iced_widget::shader::Program<crate::Message> for PreviewProgram {
                        type State = ();
                        type Primitive =
                            crate::right_sidebar::piano_waterfall::waterfall_primitive::WaterfallPrimitive;
                        fn draw(
                            &self,
                            _state: &Self::State,
                            _cursor: iced_core::mouse::Cursor,
                            _bounds: iced_core::Rectangle,
                        ) -> Self::Primitive {
                            crate::right_sidebar::piano_waterfall::waterfall_primitive::WaterfallPrimitive::new(
                                self.view.clone(),
                            )
                        }
                    }
                    iced_widget::shader::Shader::new(PreviewProgram { view })
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .into()
                } else {
                    iced_widget::container(
                        iced_widget::text("瀑布流预览（等待渲染…）")
                            .size(13)
                            .style(move |_t: &crate::Theme| iced_widget::text::Style {
                                color: Some(strong_text),
                            }),
                    )
                    .center_x(Length::Fill)
                    .center_y(Length::Fill)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
                };
            // 严格 16:9 预览卡片：几何结构由可测试的 preview_card 构建（黑底 Fixed 盒 + 居中包装），
            // 禁止 center_x/center_y(Fill)——它们会把 Fixed 覆盖成 Fill（16:9 回归根因）
            let preview_card = crate::view::video_clip::preview::preview_card(
                preview_w,
                preview_h,
                strong_color,
                base_color,
                preview_content,
            );

            let timeline = crate::view::video_clip::timeline::timeline_pane(
                &theme, total_ticks, ppq, &tempos,
            );
            let settings = {
                let s = &export_state;
                iced_widget::column![
                    iced_widget::text("导出设置（复用）")
                        .size(14)
                        .style(move |_t: &iced_core::Theme| iced_widget::text::Style {
                            color: Some(neutral_text)
                        }),
                    iced_widget::space().height(8),
                    iced_widget::text(format!(
                        "格式: {}  编码: {}  质量: {}",
                        s.container, s.codec, s.quality
                    ))
                    .size(12)
                    .style(move |_t: &iced_core::Theme| iced_widget::text::Style {
                        color: Some(weak_text)
                    }),
                    iced_widget::text(format!(
                        "分辨率: {}x{}  帧率: {}  速度: {:.1}x",
                        s.width, s.height, s.fps, s.waterfall_speed
                    ))
                    .size(12)
                    .style(move |_t: &iced_core::Theme| iced_widget::text::Style {
                        color: Some(weak_text)
                    }),
                ]
                .width(Length::Fill)
            };

            // 设置区固定高度：与 layout::SETTINGS_HEIGHT 预留严格一致，
            // 禁止 Fill——否则与预览包装器平分剩余高度，挤压预览区（16:9 回归根因之二）
            let settings_card = container(
                iced_widget::scrollable(settings)
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .width(Length::Fill)
            .height(Length::Fixed(crate::view::video_clip::layout::SETTINGS_HEIGHT))
            .padding(8)
            .style(move |_t: &iced_core::Theme| container::Style {
                background: Some(base_color.into()),
                ..Default::default()
            });

            let header = iced_widget::row![
                iced_widget::text("视频剪辑").size(18).style(move |_t: &iced_core::Theme| {
                    iced_widget::text::Style {
                        color: Some(neutral_text)
                    }
                }),
                iced_widget::space().width(Length::Fill),
                iced_widget::text(format!("缩放 {:.1}x", clip_zoom))
                    .size(12)
                    .style(move |_t: &iced_core::Theme| iced_widget::text::Style {
                        color: Some(strong_text)
                    }),
            ]
            .align_y(iced_core::Alignment::Center)
            .padding([8, 12]);

            let right_col = iced_widget::column![header, preview_card, timeline, settings_card]
                .width(Length::Fill)
                .height(Length::Fill)
                .spacing(4);

            iced_widget::row![left_panel, right_col]
                .spacing(12)
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(12)
                .into()
        })
        .into()
    }

    /// 渲染全屏瀑布流播放器（铺满主界面右侧内容区，复用右侧栏预览同款离屏渲染）。
    ///
    /// 通过 `responsive` 获取主内容区精确像素尺寸并写入 `waterfall_player.size`，
    /// 供 `Host::ensure_piano_waterfall_keyboard` 离屏定尺寸（无拉伸、无 1 帧闪现）。
    pub(crate) fn view_waterfall_player(&self) -> Element<'_> {
        puffin::profile_scope!("root_view_waterfall_player");

        let size_cell = &self.waterfall_player.size;
        let view_opt = self.waterfall_player.view.clone();

        responsive(move |size: Size| {
            let w = (size.width.max(1.0)) as u32;
            let h = (size.height.max(1.0)) as u32;
            size_cell.borrow_mut().replace((w, h));

            match &view_opt {
                Some(v) => {
                    crate::right_sidebar::piano_waterfall::waterfall_shader_element(v.clone())
                }
                None => text("（瀑布流渲染中…）")
                    .size(18)
                    .style(|theme: &Theme| text::Style {
                        color: Some(theme.extended_palette().background.neutral.text),
                    })
                    .into(),
            }
        })
        .into()
    }

    /// 渲染全屏瀑布流播放器的完整窗口布局（铺满主界面，仅瀑布流 + 键盘）。
    ///
    /// 与编辑器布局解耦：不渲染钢琴卷帘任何 UI（工具栏 / 力度面板 / 状态栏 /
    /// 卷帘画布 / 右侧栏 / 左侧轨道列表面板）。仅保留应用级全局导航栏
    /// （标题栏含模式切换退出入口、左侧 48px 路由栏），二者非钢琴卷帘界面内容。
    pub(crate) fn view_waterfall_fullscreen(&self) -> Element<'_> {
        puffin::profile_scope!("root_view_waterfall_fullscreen");

        let language = self.settings.display.language;
        let ppq = self.editor.editor_state.view.ppq;
        let note_precision = self.toolbar.note_precision.as_ticks(ppq);

        // 全局导航栏（非钢琴卷帘内容）
        let titlebar = self.titlebar.view(
            &self.window,
            self.settings.synth.use_native_titlebar,
            self.state.current_mode,
            self.state.toggle_animation.position,
            language,
            false,
        );
        let left_bar = self.sidebar.view(
            &self.window,
            language,
            self.state.current_mode,
            note_precision,
        );

        // 仅瀑布流播放器（含键盘），铺满导航栏之外的全部区域
        let player = self.view_waterfall_player();

        column![titlebar, row![left_bar, player].height(Length::Fill),].into()
    }
}
