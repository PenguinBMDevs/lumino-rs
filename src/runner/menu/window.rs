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
mod video_export;
mod video_export_handler;

impl RunnerInner {
    /// 处理窗口事件
    pub(super) fn handle_window_event(&mut self, window_event: WindowEvent) {
        match window_event {
            WindowEvent::Dialog(e) => self.handle_dialog_events(e),
            WindowEvent::Collaboration(e) => self.handle_collaboration_events(e),
            WindowEvent::Sync(e) => self.handle_sync_events(e),
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

/// 最近邻 RGBA 缩放，用于将全尺寸预览帧缩小到 dialog 可用尺寸。
/// iced_wgpu 对 >2MB 的 Handle::from_rgba 走异步 GPU 上传，
/// 每帧唯一 ID 导致缓存失效、图片永远不显示。缩小到 <2MB 走同步路径。
fn downscale_rgba(src: &[u8], sw: u32, sh: u32, tw: u32, th: u32) -> (Vec<u8>, u32, u32) {
    if tw >= sw || th >= sh || tw == 0 || th == 0 {
        return (src.to_vec(), sw, sh);
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
            dst[di..di + 4].copy_from_slice(&src[si..si + 4]);
        }
    }
    (dst, tw, th)
}
