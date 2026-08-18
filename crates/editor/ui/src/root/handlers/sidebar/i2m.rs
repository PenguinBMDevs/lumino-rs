//! 图片转 MIDI 后台转换结果轮询
//!
//! 轮询接收后台线程（`std::thread::spawn`）的转换结果 channel，
//! 成功后设置预览并强制切换到 Y 向选择工具。

use crate::root::Root;

impl Root {
    /// 确认图片转 MIDI 生成：按逐轨写入/自动建轨策略写入 document
    ///
    /// - 颜色 0 写入当前音轨；
    /// - 颜色 1+ 优先复用现有非当前音轨，数量不足时才新建缺失数量的音轨
    ///   （sidebar + document 同步扩轨）；
    pub(crate) fn poll_pending_i2m(&mut self) {
        let rx = match self.pending_i2m.as_ref() {
            Some(rx) => rx,
            None => return,
        };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(_) => return, // Empty / Disconnected
        };
        self.pending_i2m = None;
        match result {
            Ok(preview) => {
                self.editor.editor_state.image_to_midi.set_preview(preview);
                self.right_sidebar.converting = false;
                // 强制切换到 Y 向选择工具，用户用其框选生成区域
                let tool = crate::toolbar::Tool::PointerYSelect;
                self.toolbar.current_tool = tool;
                self.editor.set_tool(tool);
                self.editor
                    .invalidate_caches(lumino_ui_editor::CacheInvalidation::ALL);
                tracing::info!("图片转 MIDI 转换完成，已强制切换到 Y 向选择工具");
            }
            Err(err) => {
                self.editor.editor_state.image_to_midi.cancel();
                self.right_sidebar.converting = false;
                // 转换失败：流程结束，清除原工具记录
                self.i2m_restore_tool = None;
                tracing::error!("图片转 MIDI 转换失败: {err}");
            }
        }
    }
}
