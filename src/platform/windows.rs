use winit::window::Window;

/// 为窗口设置自定义拉伸区域（委托到 dialog crate 的 per-HWND 实现）
pub fn setup_resize_border(window: &Window) -> Result<(), String> {
    lumino_dialog::setup_resize_border(window)
}
