//! Host 模块 - UI 宿主，管理渲染和事件处理
//!
//! 该模块已拆分为以下子模块：
//! - `types`: 类型定义和工具函数
//! - `render`: 渲染逻辑（旧版 UI 和 wgpu 音符）
//! - `event`: 事件处理（窗口事件、输入）
//! - `editor_ops`: 编辑器操作（音符、洋葱皮）
//! - `dialog`: 对话框和协作功能
//! - `builder`: 构造方法（创建渲染/窗口上下文、初始化公共字段）
//! - `waterfall`: 贴图瀑布流（生成、重生成、脏区域追踪、冷静期控制）
//! - `render_thread`: 分离渲染线程管理（启停、事件通道、统计）
//!
//! 架构说明：
//! - UI线程（主线程）：处理事件、更新状态、生成渲染命令
//! - 渲染线程（独立线程）：接收命令、管理GPU资源、执行实际渲染

use std::time::Instant;

use crate::statusbar::performance::CpuMonitor;
use crate::{message, root, settings};

mod builder;
mod cache;
mod convert;
mod dialog;
mod dialog_size;
mod editor_ops;
mod event;
mod reconnect;
mod render;
mod render_ctx;
mod render_thread;
mod session_lock;
pub mod types;
mod viewport_geom;
mod waterfall;
mod window_ctx;

use render_ctx::RenderContext;
pub use window_ctx::HostWindow;
use window_ctx::WindowContext;

/// 具体窗口 → `Host` 的窗口抽象。
///
/// `Arc::clone` 是泛型调用，返回值不会在函数参数处自动 unsize；统一走这个
/// 辅助函数，调用方无需写显式类型标注。
pub fn host_window(
    window: std::sync::Arc<winit::window::Window>,
) -> std::sync::Arc<dyn HostWindow> {
    window
}

pub use cache::RenderCache;
pub use render::egui_overlay::OverlayInput;
pub use types::{DialogResult, NoteData, ShapeInsertOptions, TrackNotes};

/// UI 宿主 - 管理 旧版 渲染和 wgpu 音符渲染
///
/// 线程模型：
/// - UI线程（主线程）：处理事件、更新状态、生成渲染命令
/// - 渲染线程（独立线程）：接收命令、管理GPU资源、执行实际渲染
///
/// 架构拆分：
/// - `render_ctx`: 渲染上下文（渲染器、GPU资源、渲染线程）
/// - `window_ctx`: 窗口上下文（窗口句柄、光标、剪贴板）
/// - 直连字段：框架/全局状态（root、events、帧统计等）
pub struct Host {
    /// 渲染上下文
    pub(crate) render_ctx: RenderContext,
    /// 窗口上下文
    pub(crate) window_ctx: WindowContext,
    /// 应用状态根节点
    pub(crate) root: root::Root,
    /// egui 菜单占用区（逻辑点）。命中时点击只给 egui，不再进 旧版 队列。
    pub(crate) egui_blocked: Vec<egui::Rect>,
    /// 画布输入状态（winit / egui 直接驱动时的双击检测、拖拽标记等）。
    pub(crate) canvas_input: lumino_ui_editor::grid::GridInteractionState,
    /// 编辑器画布（卷帘 / 走带）离屏纹理视图；`Some` 时 egui 采样它作底层，
    /// 并跳过 gfx → surface 的直接 blit。
    pub(crate) egui_editor_texture: Option<std::sync::Arc<wgpu::TextureView>>,
    /// 上一帧时间
    pub(crate) last_frame_time: Instant,
    /// 上次 FPS 更新时间
    pub(crate) last_fps_update: Instant,
    /// 帧计数器
    pub(crate) frame_count: u32,
    /// 跳过 旧版 UI 渲染（性能测试用）
    pub skip_ui_rendering: bool,
    /// UI 脏标记
    pub(crate) ui_dirty: bool,
    /// 上一批事件里有控件「捕获」了输入：下一帧必须无条件重建一次。
    ///
    /// 为什么需要单独一位（而不是直接 `ui_dirty = true`）：
    /// 旧版 在基础控件捕获事件后会丢弃当前 overlay 树，新展开的下拉层要**下一次
    /// build** 才画得出来；而 `render_ui` 画完就把 `ui_dirty` 清零，同一帧里
    /// 置的脏标记会被立刻消费掉。于是「控件只改自身状态、不发消息」的交互
    /// （下拉框展开 / 滚动条抓取）可能停在「状态已变、画面没变」——
    /// 表现为点击后要再动一下鼠标（或等 Tooltip 的 500ms 定时器）才出现。
    ///
    /// 语义：捕获过输入 ⇒ 下一帧强制重建一次（一次性，消费即清零，不会自循环）。
    pub(crate) repaint_after_capture: bool,
    /// CPU 使用率监控器
    pub(crate) cpu_monitor: CpuMonitor,
    /// 上一次 GPU 帧耗时（ms）
    pub(crate) last_gpu_frame_time_ms: f32,
    /// 贴图瀑布流：有脏标记的音轨集合（编辑后需重生成）
    pub(crate) waterfall_dirty_tracks: std::collections::HashSet<u16>,
    /// 贴图瀑布流：全量配置（重生成时直接使用副本）
    pub(crate) waterfall_config: Option<lumino_gfx::TextureWaterfallConfig>,
    /// 贴图瀑布流：生成时的 MIDI 哈希（重生成时复用缓存分桶）
    pub(crate) waterfall_midi_hash: Option<String>,
    /// 贴图瀑布流：生成时的 (ppq, key_count, total_ticks)（重生成时复用）
    pub(crate) waterfall_gen_info: Option<(u16, u16, u32)>,
    /// 洋葱皮渲染后端（初始化自 `UiConfig::onion_skin_backend`；默认 GPU）。
    ///
    /// Step 3 只提供运行时开关（无常驻设置页 UI、不新增持久化写入）。
    pub(crate) onion_skin_backend: lumino_core::storage::config::OnionSkinBackend,
    /// CPU 洋葱皮运行时状态（渲染器 / 文档快照 / 帧冻结，仅 CPU 后端创建）
    pub(crate) cpu_onion_state: crate::host::render::cpu_onion::CpuOnionState,
    /// 洋葱皮后端的**会话锁**状态：只有加载过真实 MIDI/工程才锁定。
    ///
    /// 不能用 `EditorData.document.is_some()` 代替——启动/新建/关闭工程都会
    /// 由 `init_blank_project` 挂一份 2 轨空白文档，那样锁会永久为真
    /// （详见 `host::session_lock` 模块头）。
    pub(crate) midi_load: session_lock::MidiLoadLock,
    /// 消息路由器（分发消息到各处理器）
    pub(crate) message_router: root::handlers::MessageRouter,
    /// 触摸捏合回退：多指触摸点（id -> 逻辑坐标），用于无 PinchGesture 平台合成缩放
    pub(crate) active_touches: std::collections::HashMap<u64, lumino_ui_core::geom::Point>,
    /// 双指缩放的**锚点距离**（逻辑像素）。
    ///
    /// 只在「发出过一次缩放」或「手指数量变化」时前移：逐帧前移会让慢速捏合
    /// 每帧都落回阈值以下，表现为「慢慢捏合完全没反应」。
    pub(crate) pinch_anchor_distance: Option<f32>,
    /// 双指平移的上一个中点（逻辑坐标），仅在真正发出滚动增量时前移
    pub(crate) prev_pan_center: Option<lumino_ui_core::geom::Point>,
    /// 本次触摸会话是否已升级为多指手势（双指及以上出现过）
    ///
    /// 置位后即便手指降为单指也继续消费触摸，避免剩余手指的移动被当成拖拽。
    pub(crate) touch_gesture_active: bool,
    /// 触摸全部抬起后需要抑制 hover（下一批事件以 `Cursor::Unavailable` 重建）
    pub(crate) suppress_hover_after_touch: bool,
    /// 触摸长按模拟右键：首指按下点（长按抖动判定 + 右键落点）
    pub(crate) touch_press_origin: Option<lumino_ui_core::geom::Point>,
    /// 触摸长按的判定时刻（`None` = 无待判定长按）
    pub(crate) touch_long_press_deadline: Option<Instant>,
    /// 本次触摸是否已经触发过长按右键（抬手时只需释放右键）
    pub(crate) touch_long_press_fired: bool,
    /// 文本输入法是否处于激活态（焦点在 text_input 时由 旧版 打开）。
    ///
    /// 裸字母快捷键必须在此状态下让路，否则用户在输入框里打字会切工具。
    pub(crate) ime_active: bool,
    /// 旧版 定时重绘调度（Tooltip 延迟等）：待唤醒时刻（保留最早者）
    pub(crate) redraw_at: std::sync::Arc<std::sync::Mutex<Option<Instant>>>,
    /// 定时重绘后台线程是否在运行（避免重复起线程）
    pub(crate) redraw_timer_active: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Host {
    /// 获取 root 引用
    pub fn root(&self) -> &root::Root {
        &self.root
    }

    /// 获取 root 可变引用
    pub fn root_mut(&mut self) -> &mut root::Root {
        &mut self.root
    }

    /// 获取当前侧边栏音轨数量（用于推断贴图瀑布流音轨组范围）
    pub fn track_count(&self) -> usize {
        self.root.sidebar.tracks.len()
    }

    /// 获取设置面板引用
    pub fn settings(&self) -> &settings::SettingsPanel {
        self.root.settings()
    }

    /// 获取设置面板引用（可变，供 runner 注入云管理快照）
    pub fn settings_mut(&mut self) -> &mut settings::SettingsPanel {
        &mut self.root.settings
    }

    /// 取走设备检查警告窗的用户操作（无则返回 `None`）
    pub fn take_device_warning_choice(&mut self) -> Option<crate::window::DeviceWarningAction> {
        self.root.take_device_warning_action()
    }

    /// 注入 GPU 检测的完整 UI 快照（状态 + 重试门控，设置对话框专用）。
    ///
    /// 状态以 [`GpuCheckUiState`] 表达（含 `Running`），门控决定按钮可用性：
    /// 两者都由 Runner 会话状态机产出，UI 不得从详情文本里反推失败类型，
    /// 否则切换界面语言后分类立刻失效（详情是本地化后的展示文案）。
    pub fn set_gpu_check_state(
        &mut self,
        state: lumino_ui_core::state::GpuCheckUiState,
        retry_gate: lumino_ui_core::state::GpuCheckRetryGate,
    ) {
        self.root.settings.compat.check_state = state;
        self.root.settings.compat.retry_gate = retry_gate;
        self.root.settings.compat.copied = false;
        self.mark_dirty();
    }

    /// 获取云存储 UI 状态
    pub fn cloud_state(&self) -> &crate::state::cloud_state::CloudUiState {
        &self.root.cloud
    }

    /// 获取云存储 UI 状态（可变，供 runner 注入连接/列表/操作结果）
    pub fn cloud_state_mut(&mut self) -> &mut crate::state::cloud_state::CloudUiState {
        &mut self.root.cloud
    }

    /// 请求重新扫描素材库（云下载素材后调用）
    pub fn request_material_scan(&mut self) {
        self.root.request_material_scan();
    }

    /// 设置底部状态栏左侧状态消息（如保存成功后的"文件已经保存"）。
    ///
    /// `Some(msg)` 显示消息，`None` 恢复默认"就绪"。悬停工具栏按钮时
    /// 优先显示按钮解释文字，悬停离开后恢复显示此消息。
    pub fn set_status_message(&mut self, msg: Option<String>) {
        self.root.statusbar.set_status_message(msg);
    }

    /// 获取当前 wgpu 纹理格式（用于视频导出时匹配 ffmpeg `-pix_fmt`）
    pub fn texture_format(&self) -> lumino_gfx::TextureFormat {
        self.render_ctx.format
    }

    /// 调整窗口大小（同时按**当前**缩放因子重建视口）
    ///
    /// 注意这里**重新读** `scale_factor()`：DPI 变化的唯一已知收敛点就是这次重建，
    /// 因此任何"尺寸变了"的路径（`Resized`、`ScaleFactorChanged`）都必须走它——
    /// 5 个窗口的 `ScaleFactorChanged` 分支都是「取 `window.inner_size()` 后复用原有的
    /// resize 路径」，不另立入口，避免出现"改了 ui 忘了 gfx"的第二条路。
    pub fn resize(&mut self, width: u32, height: u32) {
        self.render_ctx.viewport =
            render::viewport::viewport_for(width, height, self.window_ctx.window.scale_factor());

        // 置脏：`render_ui` 在 `ui_dirty == false` 时走「仅 present 缓存帧」的
        // 早退路径（`host/render/ui.rs`），不置脏会出现"视口换了但界面还是旧布局"。
        self.ui_dirty = true;
    }

    /// 获取当前光标位置（逻辑坐标）
    pub fn cursor_position(&self) -> Option<lumino_ui_core::geom::Point> {
        self.window_ctx.cursor_position
    }

    /// 标记 UI 脏，强制下一帧重绘时重建界面（而非使用渲染缓存）。
    ///
    /// 主要用于内存监控等需要每帧重新捕获快照的对话框，在节流刷新前调用。
    pub fn mark_dirty(&mut self) {
        self.ui_dirty = true;
    }

    /// 安排一次定时重绘（保留最早时刻；复用单个后台线程）。
    pub(crate) fn schedule_redraw_at(&self, at: Instant) {
        let mut slot = match self.redraw_at.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(existing) = *slot
            && existing <= at
        {
            return;
        }
        *slot = Some(at);
        if self
            .redraw_timer_active
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            return;
        }
        drop(slot);

        let window = std::sync::Arc::clone(&self.window_ctx.window);
        let slot = std::sync::Arc::clone(&self.redraw_at);
        let active = std::sync::Arc::clone(&self.redraw_timer_active);
        let spawn_result = std::thread::Builder::new()
            .name("ui-redraw-timer".to_string())
            .spawn(move || {
                loop {
                    let target = match slot.lock() {
                        Ok(mut guard) => guard.take(),
                        Err(poisoned) => poisoned.into_inner().take(),
                    };
                    match target {
                        Some(target) => {
                            let now = Instant::now();
                            if target > now {
                                std::thread::sleep(target - now);
                            }
                            window.request_redraw();
                        }
                        None => {
                            // 持锁置空闲，与 `schedule_redraw_at` 的"写槽 + 查活跃"互斥，
                            // 避免刚置空闲又有新请求时丢唤醒。
                            let guard = match slot.lock() {
                                Ok(guard) => guard,
                                Err(poisoned) => poisoned.into_inner(),
                            };
                            if guard.is_none() {
                                active.store(false, std::sync::atomic::Ordering::SeqCst);
                                break;
                            }
                        }
                    }
                }
            });
        if let Err(error) = spawn_result {
            // 线程创建失败：必须回滚活跃标志，保证后续调度可重试起线程；
            // 否则 active 卡在 true 而线程不存在，tooltip 延迟唤醒会永久失效。
            self.redraw_timer_active
                .store(false, std::sync::atomic::Ordering::SeqCst);
            tracing::warn!("ui-redraw-timer 线程创建失败，重绘定时器暂不可用: {error}");
        }
    }

    /// 收集所有组件的内存占用快照（Root + RenderCache）
    pub fn memory_breakdown(&self) -> root::MemoryBreakdown {
        let mut breakdown = self.root.memory_breakdown();

        // 从 RenderCache 获取主音符三缓冲容量
        let (writer_cap, writer_len) = self
            .render_ctx
            .render_cache
            .note_instances_buffer
            .buffer_info(0);
        let (ready_cap, ready_len) = self
            .render_ctx
            .render_cache
            .note_instances_buffer
            .buffer_info(1);
        let (reading_cap, reading_len) = self
            .render_ctx
            .render_cache
            .note_instances_buffer
            .buffer_info(2);
        // 将三缓冲容量写入 breakdown 的附加字段
        breakdown.note_instances_writer_cap = writer_cap;
        breakdown.note_instances_writer_len = writer_len;
        breakdown.note_instances_ready_cap = ready_cap;
        breakdown.note_instances_ready_len = ready_len;
        breakdown.note_instances_reading_cap = reading_cap;
        breakdown.note_instances_reading_len = reading_len;
        breakdown.note_instance_size = std::mem::size_of::<lumino_gfx::NoteInstance>() as usize;

        breakdown
    }

    /// 路由消息：先检查直接处理，未处理则通过路由器分发
    pub(crate) fn route_message(&mut self, msg: message::Message) {
        self.root.poll_midi_input();
        self.root.poll_pending_i2m();
        self.root.poll_material_scan();
        self.root.poll_material_import();
        self.root.poll_winmm_scan();
        self.root.poll_midi_devices_scan();
        self.root.poll_path_dialogs();
        // 素材拖出跟随：鼠标已释放且未在卷帘内确认放置 → 兜底取消（避免预览残留）
        if !self.window_ctx.is_mouse_pressed {
            self.root.cancel_stale_material_follow();
        }
        if let message::Message::Batch(messages) = msg {
            for m in messages {
                self.route_message(m);
            }
            return;
        }
        if !self.root.try_handle_direct(&msg) {
            self.message_router.route(&mut self.root, msg);
        }
    }
}

/// 洋葱皮音轨调色板（按音轨索引循环取色）
///
/// 从当前调色板的第一个颜色开始取色。
fn onion_track_color(track_idx: usize) -> [u8; 4] {
    lumino_extras::palette::onion_track_color(track_idx)
}
