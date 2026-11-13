//! 视图可见性判定（egui 外壳与画布几何共用）。

use crate::root::Root;

impl Root {
    /// 右侧栏是否应随钢琴卷帘编辑区一起渲染。
    ///
    /// 右侧栏只属于钢琴卷帘编辑区：进入工程走带 / 瀑布流 / 音频视频导出面板
    /// 或关闭钢琴卷帘（钢琴卷帘 UI 隐藏）时，右侧栏跟随隐藏。
    /// 所有「非钢琴卷帘」视图（走带、瀑布流、导出面板、卷帘关闭）均不得渲染右侧栏。
    pub(crate) fn right_sidebar_visible(&self) -> bool {
        self.state.current_mode != crate::titlebar::mode_toggle::AppMode::Waterfall
            && self.sidebar.piano_roll_visible
            && !self.sidebar.is_arrangement_route()
            && !self.sidebar.audio_export_visible
            && !self.sidebar.video_export_visible
    }
}
