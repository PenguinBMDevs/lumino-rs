//! ToolbarHandler 结构体定义与主入口

use crate::message::Message;
use crate::root::Root;
use crate::root::handlers::MessageHandler;

/// 工具栏消息处理器
///
/// 注意：此处理器处理工具栏事件，但对于播放控制，
/// 它直接将消息转发给专门的处理器，而不是递归调用 update。
pub struct ToolbarHandler;

impl ToolbarHandler {
    pub fn new() -> Self {
        Self
    }

    fn handle_toolbar_event(&self, root: &mut Root, event: crate::toolbar::Event) {
        // 先让 toolbar 更新自身状态（包括自动滚动模式切换）
        root.toolbar.update(event.clone());

        // 处理播放控制 - 直接执行，不通过消息循环
        self.handle_toolbar_playback(root, &event);

        // 同步工具状态
        self.sync_toolbar_tool_state(root, &event);

        // 同步精度设置
        self.sync_toolbar_precision(root, &event);

        // 同步自动滚动模式（在 toolbar 更新之后）
        self.sync_auto_scroll_mode(root, &event);

        // 处理撤销/重做
        self.handle_toolbar_undo_redo(root, &event);

        // 处理量化
        self.handle_toolbar_quantize(root, &event);

        // 处理音符变速
        self.handle_toolbar_speed_change(root, &event);

        // 处理协作对话框
        self.handle_toolbar_collaboration(root, &event);

        // 处理内存监控对话框
        self.handle_toolbar_memory_monitor(root, &event);

        // 处理录制
        self.handle_toolbar_recording(root, &event);

        // 处理垂直翻转
        self.handle_toolbar_flip_vertical(root, &event);

        // 处理水平翻转
        self.handle_toolbar_flip_horizontal(root, &event);

        // 处理移调
        self.handle_toolbar_transpose(root, &event);

        // 处理分割/合并
        self.handle_toolbar_split_glue(root, &event);
    }
}

impl Default for ToolbarHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageHandler for ToolbarHandler {
    fn handle(&mut self, root: &mut Root, msg: Message) -> Option<Message> {
        match msg {
            Message::Toolbar(event) => {
                self.handle_toolbar_event(root, event);
                None
            }
            other => Some(other),
        }
    }
}
