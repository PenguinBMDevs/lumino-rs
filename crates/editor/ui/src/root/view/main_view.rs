//! 主视图渲染函数
//!
//! 包含 Root 主入口视图、主窗口渲染、工程走带视图和瀑布流占位页面。
//!
//! 子模块：
//! - `piano_roll`: 钢琴卷帘编辑区主视图（view_main, view_material_delete_dialog）
//! - `arrangement`: 工程走带视图（view_arrangement）
//! - `panels`: 音频/视频导出面板与瀑布流占位页面

mod arrangement;
mod panels;
mod piano_roll;
mod vertical_roll;

use crate::Element;
use crate::root::Root;

impl Root {
    /// 渲染视图（主入口，根据窗口类型分发）
    pub(super) fn root_view(&self) -> Element<'_> {
        puffin::profile_scope!("root_view");

        if self.is_progress_window {
            self.view_progress()
        } else if self.state.is_dialog_window {
            self.view_dialog()
        } else {
            self.view_main()
        }
    }

    /// 右侧栏是否应随钢琴卷帘编辑区一起渲染
    ///
    /// 右侧栏只属于钢琴卷帘编辑区：进入工程走带 / 瀑布流 / 音频视频导出面板
    /// 或关闭钢琴卷帘（钢琴卷帘 UI 隐藏）时，右侧栏跟随隐藏。
    /// 视图层调用此函数决定是否渲染右侧栏组件——所有"非钢琴卷帘"视图
    /// （走带、瀑布流、导出面板、卷帘关闭）均不得渲染右侧栏。
    pub(crate) fn right_sidebar_visible(&self) -> bool {
        self.state.current_mode != crate::titlebar::mode_toggle::AppMode::Waterfall
            && self.sidebar.piano_roll_visible
            && !self.sidebar.is_arrangement_route()
            && !self.sidebar.audio_export_visible
            && !self.sidebar.video_export_visible
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_core::storage::config::UiConfig;

    fn create_root() -> Root {
        Root::new(&UiConfig::default())
    }

    fn push_entry(root: &mut Root, name: &str) {
        root.right_sidebar
            .materials
            .entries
            .push(crate::right_sidebar::MaterialEntry {
                name: name.into(),
                author: String::new(),
                source: crate::right_sidebar::MaterialSource::User,
                path: None,
                data: None,
                multi_track: false,
                track_count: 1,
                valid: true,
                preview: None,
            });
    }

    #[test]
    fn test_material_delete_dialog_hidden_when_no_pending() {
        let root = create_root();
        assert!(root.view_material_delete_dialog().is_none());
    }

    #[test]
    fn test_material_delete_dialog_shown_when_pending() {
        let mut root = create_root();
        push_entry(&mut root, "测试素材");
        root.right_sidebar.materials.pending_delete = Some(0);
        assert!(root.view_material_delete_dialog().is_some());
    }

    #[test]
    fn test_material_delete_dialog_name_fallback_to_entry() {
        // 无快照名时回退到列表条目名称
        let mut root = create_root();
        push_entry(&mut root, "回退名");
        root.right_sidebar.materials.pending_delete = Some(0);
        let element = root.view_material_delete_dialog();
        assert!(element.is_some());
    }

    #[test]
    fn test_main_view_builds_with_delete_dialog() {
        // view_main 整体构建（含对话框叠加层路径）不 panic
        let mut root = create_root();
        push_entry(&mut root, "测试素材");
        root.right_sidebar.materials.pending_delete = Some(0);
        let _element = root.view_main();
    }
}
