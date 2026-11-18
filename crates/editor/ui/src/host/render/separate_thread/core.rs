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
//! # 画布可见性闸门（2026-11 卷帘纹理残留修复）
//!
//! 只有钢琴卷帘 / 工程走带由 gfx 离屏画布承载（`Root::editor_canvas_visible`）。
//! 其余视图（音频/视频导出面板、渲染器入口、全屏瀑布流、分组关闭后的空白态）
//! 仍逐帧发送参数——`note_data_pub` 是瀑布流 / 剪辑预览的唯一数据源，不能断流；
//! 但 `RenderParams::skip_scene_render` 会跳过最贵的离屏场景绘制，且
//! `egui_editor_texture` 置空，不再把整窗纹理合成到面板下层。
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
        frame: &wgpu::SurfaceTexture,
        egui_events: Vec<egui::Event>,
        egui_focused: bool,
    ) {
        // REND-019（PR-A §1.6）：设备已丢失（TDR）——本帧不渲染任何内容
        // （含 blit 与 egui 覆盖层），等待重连接管；避免对死设备/死渲染
        // 线程继续投递造成日志刷屏
        if lumino_gfx::device_lost() {
            return;
        }

        // 编辑器画布可见性：唯一权威源（卷帘 / 工程走带）。
        // 导出面板 / 渲染器入口 / 全屏瀑布流 / 分组关闭后的空白态都不参与合成，
        // 渲染线程侧同一枚闸门会跳过最贵的离屏场景绘制（见 build_render_params）。
        let canvas_visible = self.root.editor_canvas_visible();
        if self.render_ctx.last_editor_canvas_visible != canvas_visible {
            self.render_ctx.last_editor_canvas_visible = canvas_visible;
            // 回切首帧必须等新帧上屏：隐藏期间画布不渲染，纹理停在旧内容；
            // 置内容脏让 present 走 wait_for_frame，避免采样到旧帧。
            self.render_ctx.mark_render_content_dirty();
        }

        // 始终驱动渲染线程：note_data_pub 活体缓冲的发布发生在渲染线程处理
        // Render 命令时，瀑布流 / 剪辑预览经 take_note_data 读取。跳过调用会导致
        // 「加载 MIDI 后剪辑预览无音符，切卷帘才恢复」。
        // 画布不可见时禁用 present 滞后策略（逐帧提交，编辑事件不积压）。
        let submit = self.redraw_separate_thread(canvas_visible);

        if !canvas_visible {
            // 该分支不产出可合成的画布纹理：清掉上一帧的底层，避免整窗纹理
            // 残留在导出面板 / 渲染器面板下层继续被采样。
            self.egui_editor_texture = None;
            // 隐藏期间不存在「已提交但未呈现」的卷帘帧，收尾状态一并清空。
            self.render_ctx.pending_present_flush = None;

            if !self.skip_ui_rendering {
                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let bg = self.root.theme().palette().background;
                self.render_ui(frame, &view, Some(bg), egui_events, egui_focused);
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
            // REND-019（PR-A §1.6）：等待返回后必须复查设备丢失——置位则本帧
            // 放弃 blit（禁止向死设备提交拷贝，重连由上层接管），同时跳过
            // 上一代 覆盖层渲染（同一死设备）。
            if lumino_gfx::device_lost() {
                tracing::warn!("device lost 置位：本帧放弃 blit 与覆盖层渲染（等待重连）");
                return;
            }
            // egui 整窗绘制：把 gfx 离屏纹理交给 egui 采样，不再直接 blit 到
            // surface——否则同一帧会被写两遍（egui 随后还会整窗画一遍）。
            self.egui_editor_texture = wgpu_thread.latest_texture_view();
        }

        // 提交了「免等待」帧且登记了收尾时，渲染线程不会主动唤醒 UI：
        // 自请求一次重绘以推进到收尾帧（`Flush` 自身不再自唤醒，循环必然收敛）。
        if let RenderSubmit::Sent { wait: false, .. } = submit
            && self.render_ctx.pending_present_flush.is_some()
        {
            puffin::profile_scope!("present_flush_redraw");
            self.window_ctx.window.request_redraw();
        }

        // 上一代 UI 覆盖层渲染到同一 surface
        if !self.skip_ui_rendering {
            let view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            self.render_ui(frame, &view, None, egui_events, egui_focused);
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
            // 重绘）内容与已呈现帧等价，登记收尾只会平白多出一帧 上一代 渲染。
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
