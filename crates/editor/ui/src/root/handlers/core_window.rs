//! 核心事件与窗口事件处理器
//!
//! 处理需要直接响应的消息：
//! - `Message::Core`：转发到事件总线
//! - `Message::Window`：同步 FPS / 性能数据并更新 Window 状态

use crate::event;
use crate::root::Root;
use crate::state::root_state::DialogType;
use crate::window;

impl Root {
    /// 处理核心事件
    pub(crate) fn handle_core_event(&mut self, event: event::Event) {
        self.set_menu_open(false);
        event::emit(event);
    }

    /// 处理窗口事件
    pub(crate) fn handle_window_event(&mut self, event: window::Event) {
        let is_fps_update = matches!(&event, window::Event::FpsUpdate(_));
        let is_theme_change = matches!(&event, window::Event::Theme(_));

        if is_fps_update && let window::Event::FpsUpdate(fps) = &event {
            self.statusbar.set_fps(*fps);
        }

        // PerfUpdate 通过 Message::Window(Event::PerfUpdate) 路由到此路径，
        // 直接转发到状态栏（否则被 window.update 吞没，数据显示全零）
        if let window::Event::PerfUpdate(data) = &event {
            self.statusbar.set_perf_data(*data);
        }

        // 设备检查警告窗：勾选"不要再提示我" / 用户点击按钮
        match &event {
            window::Event::DeviceWarningSuppressToggled(checked) => {
                self.state.device_warning_suppress_checked = *checked;
            }
            window::Event::DeviceWarningActioned(action) => {
                self.state.device_warning_action = Some(*action);
            }
            _ => {}
        }

        // 设置对话框内切换主题：记录待全局应用的主题，由 Runner 逐帧取走并
        // 同步主窗口与其余对话框（切换即全局生效，见 RunnerInner::about_to_wait）。
        // 仅当主题确有变化才记录 —— Runner 回灌相同主题时不会再次入队，避免循环。
        if let window::Event::Theme(theme) = &event
            && self.state.dialog_type == DialogType::Settings
            && self.window.theme.to_string() != *theme
        {
            self.state.pending_theme_apply = Some(theme.clone());
        }

        self.window.update(event);

        if is_theme_change {
            self.editor.grid_cache.clear();
            self.editor.keyboard_cache.clear();
            self.editor.ruler_cache.clear();
        }
    }

    /// 取出设置对话框待全局应用的主题（无待应用时返回 `None`）
    ///
    /// 由 Runner 逐帧消费，消费后同步到主窗口与所有对话框。
    pub fn take_pending_theme_apply(&mut self) -> Option<String> {
        self.state.pending_theme_apply.take()
    }

    /// 取走设备检查警告窗的用户操作（供 Host / Runner 消费）
    pub fn take_device_warning_action(&mut self) -> Option<crate::window::DeviceWarningAction> {
        self.state.device_warning_action.take()
    }
}
