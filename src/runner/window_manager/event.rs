//! 主窗口事件处理：事件分派、重绘、尺寸变化。
//!
//! 从 `window_manager.rs` 拆出（REF-001 行数纪律）。

use winit::event::WindowEvent;

use super::{WindowManager, geometry};

impl WindowManager {
    /// 处理窗口事件
    pub fn handle_event(&mut self, event: WindowEvent, storage: &mut crate::storage::Storage) {
        // 点击 / 触摸优先：输入只置脏，不在事件回调里同步抢帧。
        // 一帧渲染绝不插到两次点击之间——`about_to_wait` 在事件队列排空后
        // 统一补帧，避免「狂点半天才响应」的帧饥饿回归。
        let pointer_action = matches!(
            event,
            WindowEvent::MouseInput { .. } | WindowEvent::Touch(_)
        );
        match event {
            WindowEvent::RedrawRequested => {
                self.redraw_pending = true;
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.ui.cursor_moved(position);
            }
            WindowEvent::Touch(touch) => {
                // 触控事件只更新 UI 光标位置，不处理窗口拖动
                // 窗口拖动由鼠标事件处理
                self.ui.cursor_moved(touch.location);
            }
            WindowEvent::ModifiersChanged(new_modifiers) => {
                self.modifiers = new_modifiers.state();
            }
            WindowEvent::Resized(size) => {
                self.handle_resize(size, storage);
                self.redraw_pending = true;
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                // REF-001 G-1：DPI 变化不一定伴随 `Resized`（物理尺寸可能不变）。
                // 视口里缓存着缩放因子，必须按**当前窗口状态**重建一次，否则
                // 布局按旧 DPI 换算、渲染按旧 DPI 缩放，而光标位置用的是新 DPI
                // ——出现"光标对、控件错"的半对状态。
                let size = self.window.inner_size();
                self.handle_resize(size, storage);
                self.redraw_pending = true;
            }
            WindowEvent::Moved(pos) => {
                // 更新存储的位置信息。
                // REF-001 G-2：落盘口径是**逻辑像素**（读取侧按 LogicalPosition 用），
                // 这里必须把物理坐标转换后再写，否则 150% 屏上每次重启窗口向右下漂移。
                let (x, y) = geometry::logical_position(pos, self.window.scale_factor());
                storage.ui_state.patch(|state| {
                    state.x = Some(x);
                    state.y = Some(y);
                });
            }
            WindowEvent::CloseRequested => {
                self.close_requested = true;
            }
            _ => (),
        }

        self.ui
            .handle_events(event, self.modifiers, &mut self.egui_input);
        if pointer_action {
            self.redraw_pending = true;
        }
    }

    /// 事件队列排空后统一渲染一帧（点击优先）。
    ///
    /// 点击 / 触摸只置脏，不在 `window_event` 里同步抢帧；`about_to_wait`
    /// 在所有排队事件处理完后调用本函数，保证一帧渲染不会插到两次点击之间。
    pub fn flush_pending_redraw(&mut self) {
        if !self.redraw_pending {
            return;
        }
        self.redraw_pending = false;
        // 点击优先回归诊断：`LUMINO_CLICK_DIAG=1` 时每帧打点，
        // 用于确认空闲期没有「事件 → 补帧 → 再补帧」的自旋。
        if std::env::var_os("LUMINO_CLICK_DIAG").is_some() {
            static FRAMES: std::sync::atomic::AtomicU64 =
                std::sync::atomic::AtomicU64::new(0);
            let frame = FRAMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            log::debug!("[FRAME] flush #{frame}");
        }
        self.handle_redraw();
    }

    /// 处理重绘
    fn handle_redraw(&mut self) {
        puffin::profile_function!();

        // Android 首帧：用真实窗口尺寸再配一次交换链（见字段注释）。
        // 只在首次绘制前执行，避免每帧重复 configure。
        if cfg!(target_os = "android") && !self.android_reconfigured_once {
            self.android_reconfigured_once = true;
            let size = self.window.inner_size();
            log::debug!("首帧前按真实窗口尺寸重配交换链");
            self.gfx.resize(size.width, size.height);
        }

        if self.resized {
            let size = self.window.inner_size();
            self.ui.resize(size.width, size.height);
            self.gfx.resize(size.width, size.height);
            self.resized = false;
        }

        let egui_events = self.egui_input.take();
        let egui_focused = self.egui_input.focused();
        if let Err(e) = self.gfx.with_frame(|frame, view| {
            self.ui
                .redraw_requested(frame, view, &self.gfx, egui_events, egui_focused)
        }) {
            log::warn!("帧错误: {e:?}");
            // REND-019（PR-A §1.6）：统一帧错误处置——仅可恢复错误安排重绘；
            // Lost/OOM 已置停帧标志（等重连），禁止盲重绘忙循环
            if self.gfx.handle_frame_error(&e).should_redraw() {
                self.window.request_redraw();
            }
        }
        // 本帧 egui 是否进入文本编辑：下一帧键盘事件据此决定是否消费。
        self.egui_input
            .set_wants_keyboard(self.ui.egui_wants_keyboard());
    }

    /// 处理窗口大小改变
    fn handle_resize(
        &mut self,
        size: winit::dpi::PhysicalSize<u32>,
        storage: &mut crate::storage::Storage,
    ) {
        // REND-019：几何微扰的恢复步——首次收到扰动后的 `Resized` 即恢复原
        // 几何（一次真实尺寸变化即可重建交换链/DWM 关联；一次性，见
        // [`Self::nudge_window_geometry`]）。最大化窗口在微扰时被系统退出
        // 最大化并触发本分支：此处自动恢复最大化；普通窗口恢复原尺寸。
        if let Some(target) = self.pending_size_restore
            && (target.width != size.width || target.height != size.height)
        {
            self.pending_size_restore = None;
            if self.pending_restore_maximized {
                self.pending_restore_maximized = false;
                self.window.set_maximized(true);
                // 重最大化会把主窗抬到存活对话框之上——标记一次"重新置前"
                self.raise_dialogs_pending = true;
            } else {
                let _ = self.window.request_inner_size(target);
            }
        }
        // REF-001 G-2：`Resized` 给的是**物理**尺寸，而读取侧
        // （`build_window_attributes`）按**逻辑**尺寸建窗。落盘前必须换算，
        // 否则 150% 屏上存 2160×1350 会被当逻辑值用 ⇒ 下次启动物理 3240×2025，
        // 每次重启 ×1.5。
        let (width, height) = geometry::logical_size(size, self.window.scale_factor());

        storage.ui_state.patch(|state| {
            state.w = width;
            state.h = height;
            state.is_maximized = self.window.is_maximized();
        });
        self.resized = true;
    }
}
