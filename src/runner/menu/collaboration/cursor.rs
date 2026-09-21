//! Runner 协作：光标位置与视口状态同步

use crate::runner::RunnerInner;
use crate::runner::inner::LastSentMouse;

impl RunnerInner {
    /// 同步协作状态（发送鼠标位置等）
    ///
    /// 采用变更检测：仅当内容坐标、滚动或缩放相对上次发送发生可感知变化时才入队，
    /// 避免每 50ms 无脑发送造成日志洪泛与带宽浪费。
    pub(crate) fn sync_collaboration_state(&mut self) {
        // 检查是否已连接
        let is_connected = self.collab_state.collaboration_service.is_connected();
        if !is_connected {
            return;
        }

        // 获取最新的鼠标位置（从 Host 而不是 Editor）
        let cursor_pos = self.window_state.window.ui().cursor_position();
        let editor = self.window_state.window.ui().root().editor_ref();

        let es = &editor.editor_state;

        if let Some(pos) = cursor_pos {
            // 先转换为 Canvas 视口坐标（不含滚动偏移），用于边界检查
            let viewport_pos =
                iced_core::Point::new(pos.x - es.canvas.offset_x, pos.y - es.canvas.offset_y);

            if editor.is_inside_canvas(viewport_pos) {
                // 通过边界检查后，加上滚动偏移得到内容空间坐标
                let content_pos = iced_core::Point::new(
                    viewport_pos.x + es.view.scroll_x,
                    viewport_pos.y + es.view.scroll_y,
                );

                let scroll_x = es.view.scroll_x;
                let scroll_y = es.view.scroll_y;
                let zoom_x = es.view.zoom_x;
                let zoom_y = es.view.zoom_y;

                // 变更检测：与上次发送快照比较（坐标/滚动/缩放），epsilon = 0.01
                let changed = match self.collab_state.last_sent_mouse {
                    None => true,
                    Some(prev) => {
                        (prev.x - content_pos.x).abs() > 0.01
                            || (prev.y - content_pos.y).abs() > 0.01
                            || (prev.scroll_x - scroll_x).abs() > 0.01
                            || (prev.scroll_y - scroll_y).abs() > 0.01
                            || (prev.zoom_x - zoom_x).abs() > 0.01
                            || (prev.zoom_y - zoom_y).abs() > 0.01
                    }
                };

                if !changed {
                    return;
                }

                let mouse_pos = lumino_collaboration::types::MousePosition {
                    x: content_pos.x,
                    y: content_pos.y,
                    view_state: Some(lumino_collaboration::types::ViewState {
                        scroll_x,
                        scroll_y,
                        zoom_x,
                        zoom_y,
                        ..Default::default()
                    }),
                };

                if let Err(e) = self
                    .collab_state
                    .collaboration_service
                    .send_mouse_position(mouse_pos)
                {
                    tracing::debug!("协作：发送鼠标位置失败：{}", e);
                    // 发送失败（如连接已断开）时清除快照，下次成功后再记录
                    self.collab_state.last_sent_mouse = None;
                    return;
                }

                self.collab_state.last_sent_mouse = Some(LastSentMouse {
                    x: content_pos.x,
                    y: content_pos.y,
                    scroll_x,
                    scroll_y,
                    zoom_x,
                    zoom_y,
                });
            } else {
                // 光标移出画布：清空快照，移回画布时立即重新发送首帧
                self.collab_state.last_sent_mouse = None;
            }
        }
    }
}
