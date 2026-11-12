//! 图片转 MIDI 后台转换结果轮询与收尾
//!
//! 轮询接收后台线程（`std::thread::spawn`）的转换结果 channel，
//! 成功后设置预览并强制切换到 Y 向选择工具。收尾逻辑与「缓存命中」
//! 路径共用 [`Root::finish_i2m_conversion`]，保证两条路径行为一致。

use crate::right_sidebar::I2mCacheEntry;
use crate::root::Root;
use lumino_editor_state::ImageToMidiPreview;

impl Root {
    /// 转换成功后的统一收尾：设置预览 + 切 Y 向选择工具 + 失效缓存
    ///
    /// 供后台转换完成与缓存命中两条路径复用（DRAW-030）。
    pub(crate) fn finish_i2m_conversion(&mut self, preview: ImageToMidiPreview) {
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

    /// 确认图片转 MIDI 生成：按逐轨写入/自动建轨策略写入 document
    ///
    /// - 颜色 0 写入当前音轨；
    /// - 颜色 1+ 优先复用现有非当前音轨，数量不足时才新建缺失数量的音轨
    ///   （sidebar + document 同步扩轨）。
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
                // 写入单条缓存：×/√ 之后再次转换同一图片可跳过解码/聚类/抖动
                if let Some(key) = self.pending_i2m_key.take() {
                    self.right_sidebar.i2m_cache = Some(I2mCacheEntry {
                        key,
                        preview: preview.clone(),
                    });
                }
                self.finish_i2m_conversion(preview);
            }
            Err(err) => {
                self.pending_i2m_key = None;
                self.editor.editor_state.image_to_midi.cancel();
                self.right_sidebar.converting = false;
                // 转换失败：流程结束，清除原工具记录
                self.i2m_restore_tool = None;
                tracing::error!("图片转 MIDI 转换失败: {err}");
            }
        }
    }
}
