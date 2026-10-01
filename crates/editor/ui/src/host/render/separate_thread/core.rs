//! 核心入口点 — 分离渲染线程模式的主入口、验证和参数发送
//!
//! 提供分离渲染线程模式的主入口、验证和参数发送方法。
//!
//! # present 策略（2026-10-01 滚动拖拽全程卡顿修复）
//!
//! 旧实现每帧 `send_params` 后**无条件** `wait_for_frame`。该等待只保证
//! **提交序**（渲染线程 `queue.submit` 之后即置位，见
//! `render_thread/thread.rs`），并不保证 GPU 执行完成；它的真实成本是
//! 「UI 线程必须等到渲染线程把本帧编码并提交完」，即 UI 与渲染线程的一次
//! 每帧强制会合。音符量大的场景下，渲染线程的单帧 CPU 工作（事件应用 /
//! 流式上传 / 参数准备 / 编码）会直接表现为 UI 帧的额外延迟。
//!
//! 新实现按帧类型分流：
//!
//! | 帧类型 | 提交 | present 前等待 | 说明 |
//! |---|---|---|---|
//! | 内容帧（编辑 / 洋葱皮重传 / 预览变化 / 纹理重建） | 是 | **是** | `wait_for_frame` 修复的「音符放置后不立即显示」竞态不回退 |
//! | 视口变化帧（拖拽滚动 / 缩放） | 是 | 否 | 画面最多落后一帧，数据正确性不受影响 |
//! | 视口稳定帧且存在未同步呈现的帧 | 否 | 是（`Flush`） | 收尾：把上一帧结果同步刷上屏 |
//!
//! `Flush` 是必需的收尾：非内容脏帧不再等待后会出现「提交了但没被任何一帧
//! present」的尾帧——单次 Ctrl+滚轮缩放、拖拽最后一帧都属于这种只产生一帧的
//! 视口变化，没有收尾就会永远停在离屏纹理里、表现为「滚轮没反应」。
//!
//! # 为什么不做「在途帧闸门」
//!
//! 曾计划按 GPU 在途帧数跳过提交，但 `wait_for_frame` 的完成信号是**提交**而非
//! **GPU 执行完成**（渲染线程全程无阻塞式 `device.poll(Wait)`），用它当闸门信号
//! 会永远读到 0、形同空转。真正限制「UI 领先 GPU 多少帧」的杠杆是交换链的
//! `desired_maximum_frame_latency`（见 `gfx/src/context.rs`），已在本卡一并收紧。

use crate::host::Host;
use crate::host::render_ctx::RenderViewKey;

/// 本帧的提交计划（由 [`Host::redraw_separate_thread`] 决策）。
#[derive(Clone, Copy, Debug)]
pub(super) enum RenderSubmit {
    /// 已提交本帧渲染请求；`wait` = present 前必须等待渲染线程提交本帧。
    Sent { frame_id: u64, wait: bool },
    /// 视口已稳定且存在未同步呈现的已提交帧：本帧不堆叠新渲染，
    /// 只同步等待并呈现该帧（收尾，防止最后一次视口变化永不呈现）。
    Flush { frame_id: u64 },
    /// 渲染线程不可用：不提交、不呈现、不自唤醒。
    Unavailable,
}

impl Host {
    /// 分离渲染线程模式的主渲染入口
    pub(crate) fn render_with_separate_thread(
        &mut self,
        frame: &iced_wgpu::wgpu::SurfaceTexture,
        gfx: &lumino_gfx::Context,
    ) {
        use crate::titlebar::mode_toggle::AppMode;
        use lumino_ui_core::sidebar_event::GroupId;

        // 视频剪辑面板（渲染器首级）：不应渲染钢琴卷帘的任何内容（网格/音符/标尺），
        // 仅保留瀑布流离屏预览（由 ensure_piano_waterfall_keyboard 的 is_renderer_entry 分支处理）
        // 与 iced UI，避免其他面板内容透出或 GPU 浪费。
        let is_renderer_clip = self.root.sidebar.active_group == Some(GroupId::Renderer)
            && !self.root.sidebar.audio_export_visible
            && !self.root.sidebar.video_export_visible
            && self.root.state.current_mode != AppMode::Waterfall;
        if is_renderer_clip {
            // 必须先驱动渲染线程参数发送：note_data_pub 活体缓冲的发布发生在渲染线程
            // 处理 Render 命令时，瀑布流预览经 take_note_data 读取。跳过会导致
            // 「加载 MIDI 后剪辑预览无音符，切卷帘才恢复」。
            // 该路径不做离屏拷贝，因此禁用 present 滞后策略（保持逐帧提交节奏）。
            let _ = self.redraw_separate_thread(false);

            if !self.skip_ui_rendering {
                let view = frame
                    .texture
                    .create_view(&iced_wgpu::wgpu::TextureViewDescriptor::default());
                let bg = self.root.theme().palette().background;
                self.render_iced_ui(frame, &view, Some(bg));
            }
            return;
        }

        // 全屏瀑布流播放器：与卷帘 3D 场景隔离（skip_scene_render），不发离屏拷贝；
        // 同样依赖逐帧 params 维持活体音符缓冲发布，禁用 present 滞后策略。
        let is_waterfall = self.root.state.current_mode == AppMode::Waterfall;

        // 始终驱动渲染线程：保证音符实例缓冲持续发布，供瀑布流播放器读取实时落键。
        let submit = self.redraw_separate_thread(!is_waterfall);

        if is_waterfall {
            if !self.skip_ui_rendering {
                let view = frame
                    .texture
                    .create_view(&iced_wgpu::wgpu::TextureViewDescriptor::default());
                let bg = self.root.theme().palette().background;
                self.render_iced_ui(frame, &view, Some(bg));
            }
            return;
        }

        // 将离屏渲染结果拷贝到 Surface。
        //
        // 等待策略见模块头注释：只有内容帧才必须等待渲染线程提交本帧——UI 线程与渲染
        // 线程共享同一离屏纹理与 command queue，只有等本帧 submission 入队后，UI 的
        // copy submission（FIFO）才会读到含本次编辑的最新画面；否则表现为「音符放置后
        // 不立即显示」（`wait_for_frame` 当初修复的竞态，不得回退）。
        if let Some(ref wgpu_thread) = self.render_ctx.wgpu_render_thread {
            let wait_frame = match submit {
                RenderSubmit::Sent { frame_id, wait } => wait.then_some(frame_id),
                RenderSubmit::Flush { frame_id } => Some(frame_id),
                RenderSubmit::Unavailable => None,
            };
            if let Some(frame_id) = wait_frame {
                puffin::profile_scope!("wait_for_frame");
                wgpu_thread.wait_for_frame(frame_id);
            }
            wgpu_thread.copy_offscreen_to_surface(frame, &gfx.device, &gfx.queue);
        }

        // 提交了「免等待」帧且登记了收尾时，渲染线程不会主动唤醒 UI：
        // 自请求一次重绘以推进到收尾帧（`Flush` 自身不再自唤醒，循环必然收敛）。
        if let RenderSubmit::Sent { wait: false, .. } = submit
            && self.render_ctx.pending_present_flush.is_some()
        {
            puffin::profile_scope!("present_flush_redraw");
            self.window_ctx.window.request_redraw();
        }

        // iced UI 覆盖层渲染到同一 surface
        if !self.skip_ui_rendering {
            let view = frame
                .texture
                .create_view(&iced_wgpu::wgpu::TextureViewDescriptor::default());
            self.render_iced_ui(frame, &view, None);
        }
    }

    /// 分离渲染线程模式的主渲染逻辑
    ///
    /// UI 线程只负责：
    /// 1. 更新状态
    /// 2. 生成渲染参数
    /// 3. 写入音符数据到双缓冲
    /// 4. 发送渲染参数到 WGPU 线程
    ///
    /// 注意：本函数**不做任何离屏拷贝/表面绘制**，仅发送参数（含音符实例数据）。
    /// 因此视频剪辑面板模式下也必须被调用——瀑布流预览依赖渲染线程处理
    /// Render 命令时发布活体音符缓冲。
    ///
    /// # 参数
    /// - `allow_present_lag`：是否允许「present 落后一帧」。
    ///   剪辑面板 / 瀑布流模式依赖**每帧** params 维持活体音符缓冲发布节奏，必须传
    ///   `false`（保持逐帧提交）。
    ///
    /// # 返回
    /// [`RenderSubmit`]：调用方据此决定「是否等待渲染线程」「是否自唤醒」。
    pub(super) fn redraw_separate_thread(&mut self, allow_present_lag: bool) -> RenderSubmit {
        puffin::profile_function!();
        puffin::profile_scope!("redraw_separate_thread");

        if !self.validate_render_thread_ready() {
            return RenderSubmit::Unavailable;
        }

        // 数据收集与参数构建**始终执行**：音符增量事件 / 洋葱皮流式上传 / 预览实例
        // 走独立通道，跳过它们会导致编辑延迟或瀑布流发布停摆。
        let render_data = self.collect_render_data();
        let params = self.build_render_params(render_data);
        let content_dirty = params.content_dirty;
        let view_key = RenderViewKey::from_params(&params);
        let view_changed = self.render_ctx.last_view_key != Some(view_key);
        let pending = self.render_ctx.pending_present_flush;

        // ① 收尾：视口已稳定 + 无内容变化 + 有已提交但未同步呈现的帧
        //    → 不再堆叠渲染，只同步等待并呈现该帧。
        if allow_present_lag
            && !content_dirty
            && !view_changed
            && let Some(pending_frame) = pending
        {
            puffin::profile_scope!("present_flush");
            self.render_ctx.pending_present_flush = None;
            return RenderSubmit::Flush {
                frame_id: pending_frame,
            };
        }

        // ② 提交本帧
        let Some(thread) = self.render_ctx.wgpu_render_thread.as_ref() else {
            return RenderSubmit::Unavailable;
        };
        let frame_id = thread.send_params(params);

        self.render_ctx.last_view_key = Some(view_key);
        if content_dirty {
            // 内容帧同步等待后立即呈现，已覆盖先前所有已提交帧 → 无需收尾
            self.render_ctx.pending_present_flush = None;
        } else if allow_present_lag && view_changed {
            // 仅在「视口确实变了」时登记收尾：视口未变的重复提交（如 hover 触发的
            // 重绘）内容与已呈现帧等价，登记收尾只会平白多出一帧 iced 渲染。
            self.render_ctx.pending_present_flush = Some(frame_id);
        }

        RenderSubmit::Sent {
            frame_id,
            wait: content_dirty,
        }
    }

    /// 验证渲染线程是否就绪
    pub(super) fn validate_render_thread_ready(&self) -> bool {
        if self.render_ctx.wgpu_render_thread.is_none() {
            tracing::error!("redraw_separate_thread called but wgpu_render_thread is None");
            return false;
        }

        true
    }
}
