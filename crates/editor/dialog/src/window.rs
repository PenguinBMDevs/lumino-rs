//! 对话框窗口
//!
//! 每个对话框都是独立的 winit 窗口，拥有自己的渲染上下文和 UI Host。

mod event;
mod initialize;
mod query;
mod redraw;
mod size;

use std::sync::Arc;

use lumino_core::{storage::config::UiConfig, tr};
use lumino_ui::host::DialogResult;
use lumino_ui::state::root_state::DialogType;
use winit::{
    dpi::LogicalSize,
    event_loop::ActiveEventLoop,
    window::{Window, WindowAttributes, WindowButtons},
};

/// 窗口创建期的**种子尺寸**（逻辑像素）
///
/// REF-001：per-type 尺寸硬编码已移除，真实尺寸在「入册后的首帧」由
/// [`DialogWindow::apply_measured_size`] 按内容测量决定（`window/size.rs`）。
///
/// 但隐藏窗口也必须有一个非零 client 面积：
/// - `initialize_gfx` / `initialize_ui` 对零尺寸直接报错；
/// - 阶段 3 创建 `Host` 时需要它构造初始 viewport。
///
/// 该值**不会**被用户看到（窗口 `with_visible(false)`，测量完成前不显示）；
/// 只作为测量不可用时的回退尺寸。
pub const SEED_DIALOG_SIZE: LogicalSize<f32> = LogicalSize::new(640.0, 480.0);

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
    /// 主窗口引用：用于取工作区（尺寸上限）与居中定位
    parent: Option<Arc<Window>>,
    /// 是否已完成「测量定尺寸 + 显示」（REF-001：只做一次）
    sized: bool,
}

impl DialogWindow {
    /// 获取对话框类型（供 Runner 在不引入 dialog crate 内部可见性前提下判断）
    pub fn dialog_type(&self) -> DialogType {
        self.dialog_type
    }
}

impl DialogWindow {
    /// 创建新对话框窗口（仅创建 winit 窗口，不初始化 GFX/UI）
    ///
    /// 将原本一次性的阻塞初始化拆分为多阶段：
    /// 窗口创建 → GFX 初始化 → UI 初始化 → **测量定尺寸** → 显示窗口。
    /// 这样可以避免在 `about_to_wait` 单帧中阻塞事件循环 900ms+。
    ///
    /// REF-001：本函数不再声明 per-type 尺寸——真实尺寸在「入册后的首帧」由
    /// [`DialogWindow::apply_measured_size`] 按内容测量决定；这里只用一个统一的
    /// 隐藏期种子尺寸（见 [`SEED_DIALOG_SIZE`]）。保留的 per-type 元数据只有
    /// 标题、可缩放性与 UI-004 的按钮掩码。
    pub fn new(
        event_loop: &ActiveEventLoop,
        dialog_type: DialogType,
        parent_window: Option<&Arc<Window>>,
        ui_config: &UiConfig,
    ) -> Result<Self, String> {
        puffin::profile_scope!("dialog_init_phase_window");
        let (title, resizable) = match dialog_type {
            DialogType::None => unreachable!("不会创建 None 类型的对话框"),
            DialogType::CustomPrecision => (tr!("自定义贴合"), false),
            DialogType::Collaboration => (tr!("多人协作"), false),
            DialogType::LoadConfirm => (tr!("加载大文件"), false),
            DialogType::ProjectSettings => (tr!("工程设置"), true),
            DialogType::Settings => (tr!("设置"), true),
            DialogType::SpeedChange => (tr!("变速"), false),
            DialogType::BatchEdit => (tr!("批量编辑"), true),
            DialogType::ExportProgress => (tr!("音频导出"), false),
            DialogType::VideoExport => (tr!("视频导出"), false),
            DialogType::MemoryMonitor => (tr!("内存占用详情"), false),
            DialogType::RecoverTrack => (tr!("找回删除音轨"), true),
            DialogType::SaveConfirm => ("Lumino", false),
            DialogType::CloudConnect => (tr!("连接云存储"), false),
            DialogType::CloudBrowser => (tr!("云存储文件"), true),
            DialogType::CloudNotice => (tr!("云存储提醒"), false),
            DialogType::BrushSettings => (tr!("画刷绘制行为"), false),
            DialogType::DrawSettings => (tr!("音符画设置"), false),
        };

        let mut attributes = WindowAttributes::default()
            .with_inner_size(SEED_DIALOG_SIZE)
            .with_title(title)
            .with_visible(false)
            .with_resizable(resizable)
            // UI-004：视频导出对话框内容为固定布局，去掉最大化能力（保留关闭/最小化）；
            // 其他对话框保持现状（默认全按钮）。Linux（X11/Wayland）不实现按钮掩码，
            // 但 `with_resizable(false)` 的尺寸约束在那边仍然生效。
            .with_enabled_buttons(if matches!(dialog_type, DialogType::VideoExport) {
                WindowButtons::CLOSE | WindowButtons::MINIMIZE
            } else {
                WindowButtons::all()
            });

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
            parent: parent_window.cloned(),
            sized: false,
        })
    }
}
