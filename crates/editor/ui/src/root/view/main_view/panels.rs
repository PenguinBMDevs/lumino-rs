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
        let left_bar =
            self.sidebar
                .view(&self.window, language, self.state.current_mode, note_precision);

        // 仅瀑布流播放器（含键盘），铺满导航栏之外的全部区域
        let player = self.view_waterfall_player();

        column![
            titlebar,
            row![left_bar, player].height(Length::Fill),
        ]
        .into()
    }
}
