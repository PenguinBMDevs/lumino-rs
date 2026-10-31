//! 洋葱皮每帧调度 — 同步 CPU 单轨开关、决策并分流到对应上传路径。

use crate::host::Host;
use lumino_core::storage::config::OnionSkinBackend;
use lumino_gfx::OnionSkinStreamMsg;

use super::{OnionSkinAction, OnionSkinState};

impl Host {
    /// REND-019 PR-C：重连重放标记——强制下一次洋葱皮决策走全量重建
    ///（数据从文档重放；配合 `Host::mark_gpu_replay` 的内容脏标记）。
    pub(crate) fn mark_onion_replay(&mut self) {
        self.render_ctx.onion_skin_state.force_full_next();
    }

    /// CPU 单轨模式当前是否生效（后端 = CPU 且处于钢琴卷帘渲染布局）。
    ///
    /// 走带模式仍复用同一份 GPU 音符缓冲、按洋葱皮段表 + 侧栏音轨顺序分泳道绘制
    /// 全部音轨，因此走带布局下**不启用**单轨模式（否则走带音符层会只剩当前轨）。
    /// 走带 / 纵向模式的 CPU 后端禁用由 Step 4 的设置层负责，本步只保证不破坏
    /// 既有走带渲染。
    fn cpu_current_only_active(&self) -> bool {
        self.onion_skin_backend == OnionSkinBackend::Cpu && !self.root.is_arrangement_mode()
    }

    /// 洋葱皮实例构建入口（流式分块 / 事件级增量）
    ///
    /// 每帧调用，由 [`OnionSkinState::decide_action`] 决策四种路径：
    /// - `None`：无操作（主音轨编辑豁免 + 未变化）
    /// - `Full`：全量流式会话（首次 / 布局变化）——分块构建 NoteInstance
    ///   每块 ≤ `STREAMING_CHUNK_SIZE`（800 万实例 = 128 MB），立即 send 到
    ///   WGPU 线程的 streaming channel，最后 send `Done`
    /// - `Delta`：事件级增量——只构建被编辑的洋葱皮音轨，send `TrackDelta`
    /// - `ViewState`：切轨/静音零数据重传
    ///
    /// 轨数变化（新增/删除音轨）独立于上述决策：发送 `TrackLayout` 增量
    /// 增删渲染侧段表，并为新增轨补发 `TrackDelta`（不重建既有轨）。
    ///
    /// 2026-08 单一权威源：音符数据一律从 `document` 读取（track_notes 缓存已删除）。
    ///
    /// Q3 B（CPU 后端）：GPU 洋葱皮 buffer 只驻留当前轨——全量会话只发当前轨段，
    /// 不发送 `TrackLayout`，非当前轨的区间删除增量直接丢弃（GPU 无该轨数据）。
    pub(crate) fn stream_onion_skin_instances(&mut self) {
        // 每帧同步 CPU 单轨开关：后端或渲染布局变化会强制下一次全量重建
        //（取值未变时不触发，避免每帧 Full）。
        let cpu_current_only = self.cpu_current_only_active();
        self.render_ctx
            .onion_skin_state
            .set_cpu_current_only(cpu_current_only);

        // REND-019（PR-A §1.6）：上一帧有洋葱皮消息因通道满被丢弃 → 强制
        // 全量重建补发（防增量命中残缺段表）。
        // 节流：重试 ≥500ms、告警 ≥1s——渲染线程被 GPU 拖住（TDR 前窗口）
        // 时通道会持续满，避免每帧重建风暴与日志刷屏
        //（2026-10-16 冒烟实测：空工程 ~百条 warn/秒；大工程为重建风暴）。
        if self.render_ctx.onion_skin_state.degrade_retry_ready()
            && self
                .render_ctx
                .wgpu_render_thread
                .as_ref()
                .is_some_and(|t| t.take_onion_skin_degraded())
        {
            self.render_ctx.onion_skin_state.note_degrade_retry();
            if self.render_ctx.onion_skin_state.degrade_warn_due() {
                tracing::warn!("[onion-skin] 通道满降级：强制全量重建补发（节流重试中）");
            } else {
                tracing::debug!("[onion-skin] 通道满降级重试（日志节流）");
            }
            self.render_ctx.onion_skin_state.force_full_next();
        }

        // 注：走带模式不再跳过流式上传。
        // 走带音符层直接复用钢琴卷帘常驻 GPU 音符缓冲（零第二份显存），
        // 必须让该共享缓冲在走带模式下也保持常驻（全量一次上传，编辑走增量），
        // 由渲染线程依据段表 + 侧栏音轨顺序分音轨绘制。
        // 钢琴卷帘的洋葱皮可视化绘制仍由 prepare_renderers 在走带模式提前返回而跳过。

        // CPU 单轨模式：非当前轨数据不驻留 GPU → 区间删除增量无段可作用，
        // 提前取走并丢弃（数据以 document 为准，切回 GPU 后端时全量会话重建）。
        if cpu_current_only {
            let dropped = self
                .root
                .editor
                .editor_state
                .data
                .take_pending_track_remove_ranges()
                .len();
            if dropped > 0 {
                tracing::debug!("[onion-skin] CPU 单轨模式丢弃 {dropped} 条非当前轨区间删除增量");
            }
        }

        let fp = OnionSkinState::collect_fingerprint(self);
        let action = self.render_ctx.onion_skin_state.decide_action(&fp);
        let last_track_count = self.render_ctx.onion_skin_state.last_track_count();
        let layout_changed = fp.track_count != last_track_count;
        // 待发区间删除（非当前轨）：即使决策为 None（如受影响轨全静音，
        // 洋葱皮无整轨重建目标）也必须下发——数据同步与显示豁免无关。
        let has_pending_ranges = !self
            .root
            .editor
            .editor_state
            .data
            .pending_track_remove_ranges
            .is_empty();

        if matches!(action, OnionSkinAction::None) && !layout_changed && !has_pending_ranges {
            return;
        }

        // 内容脏：本帧确有洋葱皮数据重传（全量会话 / TrackDelta / 轨布局 / 区间删除 /
        // ViewState 切轨静音）——必须在 present 前可见，走 `wait_for_frame` 等待路径。
        // 纯滚动/缩放帧在此处已提前返回，因此不会误置位。
        self.render_ctx.mark_render_content_dirty();

        // 获取 WGPU 线程引用（用于 send 消息）
        let Some(wgpu_thread) = self.render_ctx.wgpu_render_thread.as_ref() else {
            tracing::warn!("stream_onion_skin_instances: wgpu_render_thread is None");
            return;
        };

        let is_full = matches!(action, OnionSkinAction::Full);
        // 非当前轨批量删除的区间增量（`TrackRemoveRanges`）：替代整轨重建。
        // Full 会话以文档为准（已含删除结果）→ 丢弃待发区间，防二次删除。
        let pending = self
            .root
            .editor
            .editor_state
            .data
            .take_pending_track_remove_ranges();
        let pending = if is_full { Vec::new() } else { pending };
        let mut synced_tracks: Vec<usize> = Vec::with_capacity(pending.len());
        for (track_id, ranges) in pending {
            if !wgpu_thread
                .send_onion_skin_msg(OnionSkinStreamMsg::TrackRemoveRanges { track_id, ranges })
            {
                tracing::warn!("[onion-skin] 区间删除增量未送达（降级补发接管）");
                break;
            }
            synced_tracks.push(track_id);
        }
        // 当前轨段是否已重建（决定 `main_track_struct_dirty` 是否可清）：
        // 区间删除只作用于非当前轨，不参与该判定。
        let mut main_track_rebuilt = is_full || fp.current_track >= fp.track_count;
        match action {
            OnionSkinAction::Full => {
                if cpu_current_only {
                    // CPU 单轨会话：只发当前轨段（`SetViewState` 由会话内部发出，
                    // 含当前轨越界时的安全清空）
                    self.stream_onion_skin_full(&fp, wgpu_thread, Some(fp.current_track));
                } else {
                    self.stream_onion_skin_full(&fp, wgpu_thread, None);
                    // 全量会话重建后必须设置 current_track / 静音位图，否则 shader
                    // 不知道哪个段是主音轨，主音轨事件级增量也找不到段。
                    if !wgpu_thread.send_onion_skin_msg(OnionSkinStreamMsg::SetViewState {
                        current_track: fp.current_track as u32 + 1,
                        muted_tracks: fp.muted_tracks.clone(),
                    }) {
                        tracing::warn!(
                            "[onion-skin] ViewState 未送达（切轨状态可能滞后至下次同步）"
                        );
                    }
                    tracing::debug!(
                        "[onion-skin] Full 重建后同步 ViewState: current_track={}，静音 {:?}",
                        fp.current_track,
                        fp.muted_tracks
                    );
                }
            }
            other => {
                // 轨布局同步：轨数变化 → 增量增删段表 + 新增轨内容（不重建既有轨）。
                // CPU 单轨模式段表只有 1 段、且轨数变化已在决策层走 Full，此处不适用。
                if layout_changed && !cpu_current_only {
                    let sent =
                        self.stream_track_layout(wgpu_thread, fp.track_count, last_track_count);
                    main_track_rebuilt |= sent.contains(&fp.current_track);
                    synced_tracks.extend(sent);
                }
                match other {
                    OnionSkinAction::None => {}
                    OnionSkinAction::Delta(tracks) => {
                        // 去重：已由区间删除 / 新增轨布局同步的音轨不重复整轨重建
                        let filtered: Vec<usize> = tracks
                            .into_iter()
                            .filter(|t| !synced_tracks.contains(t))
                            .collect();
                        if !filtered.is_empty() {
                            main_track_rebuilt |= filtered.contains(&fp.current_track);
                            self.stream_onion_skin_delta(&fp, wgpu_thread, &filtered);
                        }
                    }
                    OnionSkinAction::ViewState => {
                        // 切轨/静音零重传：只发 ViewState uniform（当前音轨 + 静音集合）
                        if !wgpu_thread.send_onion_skin_msg(OnionSkinStreamMsg::SetViewState {
                            current_track: fp.current_track as u32 + 1,
                            muted_tracks: fp.muted_tracks.clone(),
                        }) {
                            tracing::warn!(
                                "[onion-skin] ViewState 未送达（切轨状态可能滞后至下次同步）"
                            );
                        }
                        tracing::debug!(
                            "[onion-skin] 视图状态更新：current_track={}，静音 {:?}（零数据重传）",
                            fp.current_track,
                            fp.muted_tracks
                        );
                    }
                    OnionSkinAction::Full => {
                        // Full 已在上方分支处理；保留空分支（防御性，避免 unreachable 崩溃）
                    }
                }
            }
        }

        // 标记已构建（None / Full / Delta / ViewState 四路都更新指纹，防止重复构建）
        self.render_ctx.onion_skin_state.mark_built(&fp);

        // 消费主轨结构性标记：仅当当前轨段确实已重建（或当前轨越界无段可重建）时清零
        if main_track_rebuilt {
            self.root.editor.editor_state.data.main_track_struct_dirty = false;
        }
    }
}
