//! 对话框窗口
//!
//! 每个对话框都是独立的 winit 窗口，拥有自己的渲染上下文和 UI Host。

mod event;
mod initialize;
mod query;
mod redraw;

use std::sync::Arc;

use lumino_core::storage::config::UiConfig;
use lumino_ui::host::DialogResult;
use lumino_ui::state::root_state::DialogType;
use winit::{
    dpi::LogicalSize,
    event_loop::ActiveEventLoop,
    window::{Window, WindowAttributes},
};

/// 对话框窗口
///
/// 每个对话框都是独立的窗口，有自己的渲染上下文和 UI 生命周期。
pub struct DialogWindow {
    window: Arc<Window>,
    gfx: Option<lumino_gfx::Context>,
    ui: Option<lumino_ui::Host>,
    pub(crate) dialog_type: DialogType,
    should_close: bool,
    result_data: Option<DialogResult>,
}

impl DialogWindow {
    /// 创建新对话框窗口（仅创建 winit 窗口，不初始化 GFX/UI）
    ///
    /// 将原本一次性的阻塞初始化拆分为多阶段：
    /// 窗口创建 → GFX 初始化 → UI 初始化 → 显示窗口。
    /// 这样可以避免在 `about_to_wait` 单帧中阻塞事件循环 900ms+。
    pub fn new(
        event_loop: &ActiveEventLoop,
        dialog_type: DialogType,
        _parent_window: Option<&Arc<Window>>,
        ui_config: &UiConfig,
    ) -> Result<Self, String> {
        puffin::profile_scope!("dialog_init_phase_window");
        let (width, height, title, resizable) = match dialog_type {
            DialogType::None => unreachable!("不会创建 None 类型的对话框"),
            DialogType::CustomPrecision => (480.0, 180.0, "自定义贴合", false),
            DialogType::Collaboration => (800.0, 600.0, "多人协作", false),
            DialogType::LoadConfirm => (420.0, 260.0, "加载大文件", false),
            DialogType::ProjectSettings => (450.0, 480.0, "工程设置", true),
            DialogType::Settings => (720.0, 540.0, "设置", true),
            DialogType::SpeedChange => (400.0, 250.0, "变速", false),
            DialogType::BatchEdit => (420.0, 640.0, "批量编辑", true),
            DialogType::ExportProgress => (400.0, 200.0, "音频导出", false),
            DialogType::VideoExport => (520.0, 560.0, "视频导出", false),
            DialogType::MemoryMonitor => (300.0, 440.0, "内存占用详情", false),
            DialogType::RecoverTrack => (560.0, 770.0, "找回删除音轨", true),
            DialogType::CloudConnect => (480.0, 515.0, "连接云存储", false),
            DialogType::CloudBrowser => (720.0, 520.0, "云存储文件", true),
            DialogType::CloudNotice => (440.0, 200.0, "云存储提醒", false),
        };

        let mut attributes = WindowAttributes::default()
            .with_inner_size(LogicalSize { width, height })
            .with_title(title)
            .with_visible(false)
            .with_resizable(resizable);

        // 弹窗跟随主窗口的标题栏配置；系统模式下不绘制自制标题栏。
        #[cfg(target_os = "windows")]
        {
            use winit::platform::windows::WindowAttributesExtWindows;
            attributes = if ui_config.use_native_titlebar {
                attributes.with_decorations(true)
            } else {
                attributes
                    .with_decorations(false)
                    .with_undecorated_shadow(true)
            };
        }
        #[cfg(target_os = "macos")]
        {
            use winit::platform::macos::WindowAttributesExtMacOS;
            if !ui_config.use_native_titlebar {
                attributes = attributes
                    .with_titlebar_transparent(true)
                    .with_fullsize_content_view(true);
            }
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            attributes = attributes.with_decorations(ui_config.use_native_titlebar);
        }

        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .map_err(|e| format!("创建对话框窗口失败: {e}"))?,
        );

        #[cfg(target_os = "windows")]
        if resizable && !ui_config.use_native_titlebar {
            crate::platform::windows::setup_resize_border(&window)?;
        }

        Ok(Self {
            window,
            gfx: None,
            ui: None,
            dialog_type,
            should_close: false,
            result_data: None,
        })
    }
}
