//! Runner 窗口事件处理（事件分派器）
//!
//! 大粒度的事件处理器已拆分到同级子模块，保持本文件「薄而清晰」：
//! - `dialog`：对话框类事件
//! - `audio_export` / `video_export_handler`：音频/视频导出事件
//! - `collaboration` / `sync`：协作与本地同步事件
//!
//! 共享的辅助方法（`open_dialog_traced` / `close_dialog_traced`）与
//! 预览帧缩放工具（`downscale_rgba`）保留在本文件中。

use crate::runner::{RunnerInner, dialog_manager::DialogType};
use lumino_ui::event::window::Event as WindowEvent;

mod audio_export;
mod collaboration;
mod dialog;
mod sync;
mod track;
mod video_export;
mod video_export_handler;

impl RunnerInner {
    /// 处理窗口事件
    pub(super) fn handle_window_event(&mut self, window_event: WindowEvent) {
        match window_event {
            WindowEvent::Dialog(e) => self.handle_dialog_events(e),
            WindowEvent::Collaboration(e) => self.handle_collaboration_events(e),
            WindowEvent::Sync(e) => self.handle_sync_events(e),
            WindowEvent::Track(e) => self.handle_track_events(e),
            _ => {}
        }
    }

    /// 打开对话框并记录日志（保持「先日志后打开」顺序）。
    fn open_dialog_traced(&mut self, dialog: DialogType, label: &str) {
        tracing::info!("请求打开{label}对话框");
        self.window_state.dialog_manager.open_dialog(dialog);
    }

    /// 关闭对话框并记录日志（保持「先关闭后日志」顺序）。
    fn close_dialog_traced(&mut self, dialog: DialogType, label: &str) {
        self.window_state
            .dialog_manager
            .mark_dialog_for_close(dialog);
        tracing::info!("请求关闭{label}对话框");
    }
}

/// 最近邻缩放 + BGRA→RGBA 交换（零拷贝预览路径）。
///
/// 输入为 BGRA 格式，输出为 RGBA 格式。
/// 等价于先 `downscale_rgba` 再 `swap(0,2)`，但只需一次内存分配
/// 且避免全帧 clone（约 8MB@1080p）。
pub(super) fn downscale_bgra_to_rgba(
    src: &[u8],
    sw: u32,
    sh: u32,
    tw: u32,
    th: u32,
) -> (Vec<u8>, u32, u32) {
    if tw >= sw || th >= sh || tw == 0 || th == 0 {
        // 不需缩小：直接 clone 并在 clone 中交换
        let mut dst = src.to_vec();
        for pixel in dst.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        return (dst, sw, sh);
    }
    let mut dst = vec![0u8; (tw * th * 4) as usize];
    for dy in 0..th {
        let sy = (dy as f64 * sh as f64 / th as f64) as u32;
        let src_row = (sy * sw * 4) as usize;
        let dst_row = (dy * tw * 4) as usize;
        for dx in 0..tw {
            let sx = (dx as f64 * sw as f64 / tw as f64) as u32;
            let si = src_row + (sx * 4) as usize;
            let di = dst_row + (dx * 4) as usize;
            // BGRA → RGBA: 读取 B,G,R,A → 写入 R,G,B,A
            dst[di] = src[si + 2]; // R
            dst[di + 1] = src[si + 1]; // G
            dst[di + 2] = src[si]; // B
            dst[di + 3] = src[si + 3]; // A
        }
    }
    (dst, tw, th)
}
