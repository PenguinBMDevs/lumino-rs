//! 音频/视频导出面板与瀑布流全屏播放器

use iced_core::{Length, Size};
use iced_widget::{Space, column, container, responsive, row};

use lumino_core::{tr, trf};

use crate::root::Root;
use crate::view::video_clip::preview::{PreviewGate, preview_gate, preview_placeholder};
use crate::{Element, Theme};

impl Root {
    /// 渲染音频渲染面板（在主界面钢琴卷帘区域显示）。
    ///
    /// 内容已**完全**由 egui 绘制（`audio_export::show_panel`，含参数表 / 进度 /
    /// 暂停与取消）。iced 侧只保留铺满内容区的占位，原 `view/audio_export_dialog.rs`
    /// 已删除。
    pub(crate) fn view_audio_export_panel(&self) -> Element<'_> {
        puffin::profile_scope!("root_view_audio_export_panel");
        Space::new().width(Length::Fill).height(Length::Fill).into()
    }

    /// 渲染视频渲染面板（在主界面钢琴卷帘区域显示）。
    ///
    /// 整块由 egui 绘制（`VideoExportPanelModel`）；iced 只保留同尺寸底色占位，
    /// 避免双层表单与双重响应。
    pub(crate) fn view_video_export_panel(&self) -> Element<'_> {
        puffin::profile_scope!("root_view_video_export_panel");

        let palette = self.window.theme.extended_palette();
        container(
            iced_widget::Space::new()
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(palette.background.base.color)),
            ..Default::default()
        })
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
        // 真实内容长度：优先文档轨尾标（与播放停止点同源），空工程回退画布默认
        let total_ticks = self.clip_real_total_ticks();
        let ppq = self.editor.editor_state.view.ppq;
        let tempos: Vec<(u32, f32)> = self.tempo_pairs();
        // 剪辑面板独立传输时钟（秒域，与卷帘 PlaybackManager 完全无关）
        let playhead_secs = self.state.video_clip.clip_position_secs;
        let is_playing = self.state.video_clip.clip_playing;
        let video_edit = self.state.video_clip.video_edit;
        let audio_edit = self.state.video_clip.audio_edit;

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
            // 时间轴可视宽度（剪辑带滚动条/滚动的钳制基准，随消息携带）
            let timeline_viewport_w =
                crate::view::video_clip::layout::renderer_panel_available_width(size);

            // 左侧轨道占位（后续接入真实轨道列表）
            let left_panel = container(
                iced_widget::column![
                    iced_widget::text(tr!("轨道"))
                        .size(12)
                        .style(move |_t: &iced_core::Theme| {
                            iced_widget::text::Style {
                                color: Some(weak_text),
                            }
                        }),
                    iced_widget::space().height(8),
                    iced_widget::text(tr!("（轨道列表占位）")).size(11).style(
                        move |_t: &iced_core::Theme| iced_widget::text::Style {
                            color: Some(strong_text)
                        }
                    ),
                ]
                .padding(12),
            )
            .width(Length::Fixed(
                crate::view::video_clip::layout::LEFT_RESERVED,
            ))
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

            // 预览内容：**播放指示线压在素材带上**才显示瀑布流；
            // 在带内但纹理未就绪 → 等待占位；不在带内 → 明确提示当前位置无素材。
            // 判定与 Host 渲染侧同源（`position_in_video_window`），不会一个显示一个不显示。
            let playhead_in_band = self
                .state
                .video_clip
                .position_in_video_window(self.clip_real_duration_secs() as f32);
            let texture_ready = waterfall_view.is_some();
            let preview_content: crate::Element<'_> =
                match preview_gate(playhead_in_band, texture_ready) {
                    PreviewGate::NoMaterial => preview_placeholder(
                        tr!("播放指示线不在视频带上（当前位置无素材）"),
                        strong_text,
                    ),
                    PreviewGate::WaitingRender => {
                        preview_placeholder(tr!("瀑布流预览（等待渲染…）"), strong_text)
                    }
                    // 瀑布流画面由 egui 用同一张离屏纹理铺放（探针给出实测矩形）；
                    // iced 只留铺满黑盒的空内容，避免同一纹理被采样两次。
                    PreviewGate::Waterfall => {
                        Space::new().width(Length::Fill).height(Length::Fill).into()
                    }
                };
            // 实测内容区矩形：egui 侧按它铺瀑布流纹理（布局常量正推会错位）。
            let preview_content =
                crate::view::video_clip::rect_probe::probe("renderer::preview", preview_content);
            // 严格 16:9 预览卡片：几何结构由可测试的 preview_card 构建（黑底 Fixed 盒 + 居中包装），
            // 禁止 center_x/center_y(Fill)——它们会把 Fixed 覆盖成 Fill（16:9 回归根因）
            let preview_card = crate::view::video_clip::preview::preview_card(
                preview_w,
                preview_h,
                strong_color,
                base_color,
                preview_content,
            );

            let timeline = crate::view::video_clip::rect_probe::probe(
                "renderer::timeline",
                crate::view::video_clip::timeline::timeline_pane(
                    &theme,
                    crate::view::video_clip::timeline::TimelinePaneParams {
                        total_ticks,
                        ppq,
                        tempos: &tempos,
                        clip: &self.state.video_clip,
                        viewport_w: timeline_viewport_w,
                        playhead_secs,
                        is_playing,
                        video_edit,
                        audio_edit,
                    },
                ),
            );
            let settings =
                {
                    let s = &export_state;
                    iced_widget::column![
                        iced_widget::text(tr!("导出设置（复用）")).size(14).style(
                            move |_t: &iced_core::Theme| iced_widget::text::Style {
                                color: Some(neutral_text)
                            }
                        ),
                        iced_widget::space().height(8),
                        iced_widget::text(trf!(
                            "格式: {container}  编码: {codec}  质量: {quality}",
                            container = s.container,
                            codec = s.codec,
                            // `quality` 是规范数据值（高/中/低），显示层单独取词
                            quality = lumino_core::i18n::tr_dyn(&s.quality)
                        ))
                        .size(12)
                        .style(move |_t: &iced_core::Theme| iced_widget::text::Style {
                            color: Some(weak_text)
                        }),
                        iced_widget::text(trf!(
                            "分辨率: {width}x{height}  帧率: {fps}  速度: {speed}x",
                            width = s.width,
                            height = s.height,
                            fps = s.fps,
                            speed = format!("{:.1}", s.waterfall_speed)
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
            .height(Length::Fixed(
                crate::view::video_clip::layout::SETTINGS_HEIGHT,
            ))
            .padding(8)
            .style(move |_t: &iced_core::Theme| container::Style {
                background: Some(base_color.into()),
                ..Default::default()
            });

            let header = iced_widget::row![
                iced_widget::text(tr!("视频剪辑"))
                    .size(18)
                    .style(move |_t: &iced_core::Theme| {
                        iced_widget::text::Style {
                            color: Some(neutral_text),
                        }
                    }),
                iced_widget::space().width(Length::Fill),
                iced_widget::text(trf!("缩放 {zoom}x", zoom = format!("{:.1}", clip_zoom)))
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

        responsive(move |size: Size| {
            let w = (size.width.max(1.0)) as u32;
            let h = (size.height.max(1.0)) as u32;
            size_cell.borrow_mut().replace((w, h));

            // 画面由 egui 铺放同一张离屏纹理（探针给出实测矩形）；iced 只留空内容。
            // 纹理未就绪时 egui 不画，露出本层底色（与 iced 原先的深底一致）。
            let content: crate::Element<'_> =
                Space::new().width(Length::Fill).height(Length::Fill).into();
            crate::view::video_clip::rect_probe::probe("waterfall::player", content)
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
        let note_precision = self.toolbar.snap_spec.ticks(ppq) as f32;

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
            self.mixer_panel.open,
        );

        // 仅瀑布流播放器（含键盘），铺满导航栏之外的全部区域
        let player = self.view_waterfall_player();

        column![titlebar, row![left_bar, player].height(Length::Fill),].into()
    }
}
