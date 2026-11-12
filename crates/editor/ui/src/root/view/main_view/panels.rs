//! 音频/视频导出面板与瀑布流全屏播放器。
//!
//! 三块内容（音频导出 / 视频导出 / 渲染器入口面板）与全屏瀑布流播放器都已由
//! egui 整块绘制：
//! - 渲染器入口面板的几何由 `root::video_clip_math::renderer_panel_layout` 计算，
//!   iced 侧不再需要 `rect_probe`；
//! - 离屏纹理尺寸也由 egui 模型构建时写回 `waterfall_player.size`。
//!
//! iced 只保留**铺满内容区**的占位，避免与 egui 双层绘制。

use iced_core::Length;
use iced_widget::Space;

use crate::Element;
use crate::root::Root;

impl Root {
    /// 音频渲染面板占位（内容由 `audio_export::show_panel` 整块绘制）。
    pub(crate) fn view_audio_export_panel(&self) -> Element<'_> {
        puffin::profile_scope!("root_view_audio_export_panel");
        fill()
    }

    /// 视频渲染面板占位（内容由 `video_export::show_panel` 整块绘制）。
    pub(crate) fn view_video_export_panel(&self) -> Element<'_> {
        puffin::profile_scope!("root_view_video_export_panel");
        fill()
    }

    /// 渲染器入口面板占位（轨道 / 16:9 预览 / 时间轴 / 导出设置全由 egui 绘制）。
    pub(crate) fn view_renderer_panel(&self) -> Element<'_> {
        puffin::profile_scope!("root_view_renderer_panel");
        fill()
    }

    /// 全屏瀑布流播放器占位（纹理由 egui 铺放，尺寸由模型构建时写回）。
    pub(crate) fn view_waterfall_player(&self) -> Element<'_> {
        puffin::profile_scope!("root_view_waterfall_player");
        fill()
    }

    /// 渲染全屏瀑布流播放器的完整窗口布局（铺满主界面，仅瀑布流 + 键盘）。
    ///
    /// 仅保留应用级全局导航栏（标题栏 + 左侧 48px 路由栏），二者非钢琴卷帘内容。
    pub(crate) fn view_waterfall_fullscreen(&self) -> Element<'_> {
        puffin::profile_scope!("root_view_waterfall_fullscreen");

        let language = self.settings.display.language;
        let titlebar = self.titlebar.view(
            &self.window,
            self.settings.synth.use_native_titlebar,
            self.state.current_mode,
            self.state.toggle_animation.position,
            language,
            false,
        );
        let left_bar = self.sidebar.view();
        let player = self.view_waterfall_player();

        iced_widget::column![
            titlebar,
            iced_widget::row![left_bar, player].height(Length::Fill),
        ]
        .into()
    }
}

/// 铺满父容器的透明占位。
fn fill<'a>() -> Element<'a> {
    Space::new().width(Length::Fill).height(Length::Fill).into()
}
