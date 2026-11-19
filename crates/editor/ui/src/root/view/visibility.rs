//! 视图可见性判定（egui 外壳与画布几何共用）。

use crate::root::Root;

impl Root {
    /// 主区域当前是否真的在显示 gfx 编辑器画布（钢琴卷帘 / 工程走带）。
    ///
    /// **白名单语义**：只有这两个视图由 gfx 离屏画布承载。音频/视频导出面板、
    /// 渲染器入口、全屏瀑布流、分组关闭后的空白态一律为 `false`——新增主区域
    /// 面板时必须显式加进来，避免「新面板忘了关画布」的同类问题复发。
    ///
    /// 与 [`Self::right_sidebar_visible`] 的区别：后者只描述右侧栏归属，
    /// 工程走带下为 `false`，不能拿来当画布可见性。渲染线程的
    /// `skip_scene_render` 与 UI 侧的底层纹理合成都只认本函数。
    pub(crate) fn editor_canvas_visible(&self) -> bool {
        self.state.current_mode != crate::titlebar::mode_toggle::AppMode::Waterfall
            && (self.sidebar.is_arrangement_route() || self.sidebar.is_piano_roll_panel())
    }

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

    /// 主区域是否没有任何主视图（「无面板」默认第一屏的显示条件）。
    ///
    /// 与 [`Self::editor_canvas_visible`] 互补的白名单语义：只有画布 / 走带 /
    /// 渲染器入口 / 音视频导出面板都未占用主区域时，才允许显示品牌第一屏。
    /// 新增主区域面板时必须同步扩展这里的排除列表，否则第一屏会和面板叠在一起。
    pub(crate) fn shows_first_screen(&self) -> bool {
        self.state.current_mode == crate::titlebar::mode_toggle::AppMode::Editor
            && !self.state.is_dialog_window
            && !self.is_progress_window
            && !self.is_device_warning_window
            && !self.editor_canvas_visible()
            && !self.is_renderer_entry_active()
            && !self.sidebar.audio_export_visible
            && !self.sidebar.video_export_visible
    }
}
