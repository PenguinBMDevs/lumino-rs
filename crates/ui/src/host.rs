//! Host 模块 - UI 宿主，管理渲染和事件处理
//!
//! 该模块已拆分为以下子模块：
//! - `types`: 类型定义和工具函数
//! - `render`: 渲染逻辑（iced UI 和 wgpu 音符）
//! - `event`: 事件处理（窗口事件、输入）
//! - `editor_ops`: 编辑器操作（音符、洋葱皮）
//! - `dialog`: 对话框和协作功能
//!
//! 架构说明：
//! - UI线程（主线程）：处理事件、更新状态、生成渲染命令
//! - 渲染线程（独立线程）：接收命令、管理GPU资源、执行实际渲染

use std::sync::{Arc, OnceLock};
use std::time::Instant;

use iced_core::{Font, Size};
use iced_wgpu::graphics::Viewport;
use iced_winit::winit;

use crate::statusbar::performance::CpuMonitor;
use crate::{WgpuRenderThread, config, root, settings};

mod cache;
mod dialog;
mod editor_ops;
mod event;
mod render;
mod render_ctx;
pub mod types;
mod window_ctx;

use render_ctx::{RenderContext, WgpuResources};
use window_ctx::WindowContext;

pub use cache::RenderCache;
pub use types::{DialogResult, NoteData, TrackNotes};

/// UI 宿主 - 管理 iced 渲染和 wgpu 音符渲染
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
    /// 事件列表
    pub(crate) events: Vec<iced_core::Event>,
    /// 上一帧时间
    pub(crate) last_frame_time: Instant,
    /// 上次 FPS 更新时间
    pub(crate) last_fps_update: Instant,
    /// 帧计数器
    pub(crate) frame_count: u32,
    /// 跳过 Iced UI 渲染（性能测试用）
    pub skip_ui_rendering: bool,
    /// UI 脏标记
    pub(crate) ui_dirty: bool,
    /// CPU 使用率监控器
    pub(crate) cpu_monitor: CpuMonitor,
    /// 上一次 GPU 帧耗时（ms）
    pub(crate) last_gpu_frame_time_ms: f32,
    /// 滚动速度追踪器（用于 overscan 计算）
    pub(crate) scroll_tracker: render::note_worker::ScrollVelocityTracker,
    /// 高精度贴图：有脏标记的音轨集合（编辑后需重生成）
    pub(crate) hires_dirty_tracks: std::collections::HashSet<u16>,
    /// 高精度贴图：脏区域追踪（track_idx → 脏音符列表），用于临时贴图覆层
    pub(crate) hires_dirty_regions: std::collections::HashMap<u16, Vec<lumino_gfx::OnionSkinNote>>,
    /// 高精度贴图：最后一次编辑时间（用于冷静期判断）
    pub(crate) hires_last_edit: Option<Instant>,
    /// 高精度贴图：全量配置（重生成时直接使用副本）
    pub(crate) hires_config: Option<lumino_gfx::HiResConfig>,
    /// 高精度贴图：生成时的 MIDI 哈希（重生成时复用缓存分桶）
    pub(crate) hires_midi_hash: Option<String>,
    /// 高精度贴图：生成时的 (ppq, key_count, total_ticks)（重生成时复用）
    pub(crate) hires_gen_info: Option<(u16, u16, u32)>,
}

impl Host {
    /// 创建渲染上下文和窗口上下文（三个构造函数的公共逻辑）
    fn create_render_and_window_context(
        window: Arc<winit::window::Window>,
        width: u32,
        height: u32,
        ui_config: &config::UiConfig,
        gfx: &lumino_gfx::Context,
    ) -> (RenderContext, WindowContext) {
        let viewport =
            Viewport::with_physical_size(Size::new(width, height), window.scale_factor() as f32);

        let font = create_font_from_config(ui_config);

        let note_renderer = lumino_gfx::NoteRenderer::new(&gfx.device, &gfx.queue, gfx.format);
        let grid_renderer = lumino_gfx::GridRenderer::new(&gfx.device, gfx.format);

        let wgpu_resources = WgpuResources {
            device: gfx.device.clone(),
            queue: gfx.queue.clone(),
            format: gfx.format,
            adapter: gfx.adapter.clone(),
        };
        let render_ctx = RenderContext::new(
            &wgpu_resources,
            viewport,
            note_renderer,
            grid_renderer,
            font,
        );

        (render_ctx, WindowContext::new(window))
    }

    /// 创建 Host 公共字段（三个构造函数的公共 Self 初始化）
    ///
    /// 干净启动时即初始化默认高精度洋葱皮上下文，确保无 MIDI 文件时
    /// 编辑音符也能触发贴图生成。
    fn new_common_fields(
        render_ctx: RenderContext,
        window_ctx: WindowContext,
        root: root::Root,
        ui_config: &config::UiConfig,
    ) -> Self {
        let key_count = if ui_config.enable_256key { 256 } else { 128 };
        let hires_config = lumino_gfx::HiResConfig {
            enabled: ui_config.hires_onion_enabled,
            measures_per_group: ui_config.hires_measures_per_group,
            tile_width_px: ui_config.hires_tile_width_px,
            cooldown_secs: ui_config.hires_cooldown_secs,
            gpu_mem_limit_mb: ui_config.hires_gpu_mem_limit_mb,
            group_tile_mem_limit_mb: 256,
            cache_dir: lumino_gfx::HiResConfig::default().cache_dir,
        };
        let midi_hash = lumino_gfx::compute_midi_hash(b"empty-project");
        let ppq = lumino_core::view_state::DEFAULT_PPQ;
        let total_ticks = lumino_core::view_state::DEFAULT_TOTAL_TICKS;

        Self {
            render_ctx,
            window_ctx,
            root,
            events: Vec::new(),
            last_frame_time: Instant::now(),
            last_fps_update: Instant::now(),
            frame_count: 0,
            skip_ui_rendering: false,
            ui_dirty: false,
            cpu_monitor: CpuMonitor::new(),
            last_gpu_frame_time_ms: 0.0,
            scroll_tracker: render::note_worker::ScrollVelocityTracker::new(),
            hires_dirty_tracks: std::collections::HashSet::new(),
            hires_dirty_regions: std::collections::HashMap::new(),
            hires_last_edit: None,
            hires_config: Some(hires_config),
            hires_midi_hash: Some(midi_hash),
            hires_gen_info: Some((ppq, key_count, total_ticks)),
        }
    }

    /// 创建新的 Host
    pub fn new(
        window: Arc<winit::window::Window>,
        width: u32,
        height: u32,
        ui_config: &config::UiConfig,
        gfx: &lumino_gfx::Context,
        is_progress: bool,
    ) -> Self {
        let (render_ctx, window_ctx) =
            Self::create_render_and_window_context(window, width, height, ui_config, gfx);
        let root = if is_progress {
            root::Root::new_progress(&ui_config.theme)
        } else {
            root::Root::new(ui_config)
        };
        Self::new_common_fields(render_ctx, window_ctx, root, ui_config)
    }

    /// 创建对话框 Host
    pub fn new_dialog(
        window: Arc<winit::window::Window>,
        width: u32,
        height: u32,
        ui_config: &config::UiConfig,
        gfx: &lumino_gfx::Context,
        dialog_type: crate::state::root_state::DialogType,
    ) -> Self {
        let (render_ctx, window_ctx) =
            Self::create_render_and_window_context(window, width, height, ui_config, gfx);
        let root = root::Root::new_dialog(&ui_config.theme, dialog_type);
        Self::new_common_fields(render_ctx, window_ctx, root, ui_config)
    }

    /// 创建设置对话框 Host（使用主窗口的配置）
    pub fn new_settings_dialog(
        window: Arc<winit::window::Window>,
        width: u32,
        height: u32,
        ui_config: &config::UiConfig,
        gfx: &lumino_gfx::Context,
    ) -> Self {
        let (render_ctx, window_ctx) =
            Self::create_render_and_window_context(window, width, height, ui_config, gfx);
        let root = root::Root::new_settings_dialog(&ui_config.theme, ui_config);
        Self::new_common_fields(render_ctx, window_ctx, root, ui_config)
    }

    /// 启用真正的分离渲染线程（新架构）
    ///
    /// 这会将所有 WGPU 渲染（音符、网格、键盘、标尺）从 UI 线程完全分离
    pub fn enable_separate_render_thread(&mut self) {
        if self.render_ctx.wgpu_render_thread.is_some() {
            return;
        }

        // 创建音符事件通道（tx 不再存储，由 render thread 持有 rx）
        let (_tx, rx) = std::sync::mpsc::channel();

        // 启动 WGPU 渲染线程
        match WgpuRenderThread::spawn(
            self.render_ctx.device.clone(),
            self.render_ctx.queue.clone(),
            self.render_ctx.format,
            rx,
            Arc::clone(&self.render_ctx.render_cache.note_instances_buffer),
        ) {
            Ok(thread) => {
                self.render_ctx.wgpu_render_thread = Some(thread);
                self.render_ctx.use_separate_render_thread = true;
                tracing::info!("Host: Separate WGPU render thread enabled");
            }
            Err(e) => {
                tracing::error!("Host: Failed to start separate render thread: {}", e);
            }
        }
    }

    /// 禁用分离渲染线程
    pub fn disable_separate_render_thread(&mut self) {
        if let Some(thread) = self.render_ctx.wgpu_render_thread.take() {
            thread.shutdown();
            self.render_ctx.use_separate_render_thread = false;
            tracing::info!("Host: Separate WGPU render thread disabled");
        }
    }

    /// 获取分离渲染线程统计
    pub fn separate_render_stats(&self) -> Option<crate::WgpuRenderStats> {
        self.render_ctx
            .wgpu_render_thread
            .as_ref()
            .map(|t| t.stats())
    }

    /// 启动高精度洋葱皮贴图生成（MIDI 加载后调用）
    pub fn generate_hires_onion_skin(
        &mut self,
        notes: Vec<Vec<lumino_gfx::OnionSkinNote>>,
        ppq: u16,
        key_count: u16,
        total_ticks: u32,
        config: lumino_gfx::HiResConfig,
        midi_hash: String,
    ) {
        // 存下上下文供重生成使用
        self.hires_midi_hash = Some(midi_hash.clone());
        self.hires_gen_info = Some((ppq, key_count, total_ticks));
        self.hires_config = Some(config.clone());
        if let Some(ref thread) = self.render_ctx.wgpu_render_thread {
            thread.send_control(
                lumino_gfx::render_thread::ControlCommand::GenerateHiResOnionSkin {
                    notes,
                    ppq,
                    key_count,
                    total_ticks,
                    config,
                    midi_hash,
                },
            );
        }
    }

    /// 释放高精度洋葱皮资源（关闭 MIDI / 新建工程时调用）
    ///
    /// 释放 GPU 资源后，根据当前编辑器视图状态重新初始化默认上下文，
    /// 保证干净启动或关闭文件后仍能继续编辑并生成贴图。
    pub fn dispose_hires_onion_skin(&mut self) {
        if let Some(ref thread) = self.render_ctx.wgpu_render_thread {
            thread.send_control(lumino_gfx::render_thread::ControlCommand::DisposeHiResOnionSkin);
        }
        self.hires_dirty_tracks.clear();
        self.hires_dirty_regions.clear();
        self.hires_last_edit = None;
        self.init_default_hires_context();
    }

    /// 根据当前编辑器视图状态初始化默认高精度洋葱皮上下文
    ///
    /// 无 MIDI 文件时（干净启动 / 新建工程）使用 editor 的默认 ppq/key_count/total_ticks。
    fn init_default_hires_context(&mut self) {
        let view = &self.root.editor.editor_state.view;
        let key_count = view.key_count;
        let ppq = view.ppq;
        let total_ticks = view.total_ticks;
        let ui_cfg = self.hires_config.clone().unwrap_or_else(|| {
            let default = lumino_gfx::HiResConfig::default();
            lumino_gfx::HiResConfig {
                enabled: default.enabled,
                measures_per_group: default.measures_per_group,
                tile_width_px: default.tile_width_px,
                cooldown_secs: default.cooldown_secs,
                gpu_mem_limit_mb: default.gpu_mem_limit_mb,
                group_tile_mem_limit_mb: default.group_tile_mem_limit_mb,
                cache_dir: default.cache_dir,
            }
        });
        let config = lumino_gfx::HiResConfig {
            enabled: ui_cfg.enabled,
            measures_per_group: ui_cfg.measures_per_group,
            tile_width_px: ui_cfg.tile_width_px,
            cooldown_secs: ui_cfg.cooldown_secs,
            gpu_mem_limit_mb: ui_cfg.gpu_mem_limit_mb,
            group_tile_mem_limit_mb: ui_cfg.group_tile_mem_limit_mb,
            cache_dir: ui_cfg.cache_dir,
        };
        let midi_hash = lumino_gfx::compute_midi_hash(b"empty-project");
        self.hires_config = Some(config);
        self.hires_midi_hash = Some(midi_hash);
        self.hires_gen_info = Some((ppq, key_count, total_ticks));
    }

    /// 发送高精度贴图重生命令（冷静期到期后由 runner 调用）
    ///
    /// `group_notes` 需包含该 `track_idx` 所在 track_group 的所有音轨音符，
    /// runner 将使用这些最新数据重新合并 group tile，避免读取过期缓存。
    #[allow(clippy::too_many_arguments)]
    pub fn send_hires_regen(
        &mut self,
        track_idx: u16,
        group_notes: Vec<Vec<lumino_gfx::OnionSkinNote>>,
        ppq: u16,
        key_count: u16,
        total_ticks: u32,
        track_count: u16,
        config: lumino_gfx::HiResConfig,
        midi_hash: String,
    ) {
        if let Some(ref thread) = self.render_ctx.wgpu_render_thread {
            thread.send_control(
                lumino_gfx::render_thread::ControlCommand::RegenerateHiResTrack {
                    track_idx,
                    group_notes,
                    ppq,
                    key_count,
                    total_ticks,
                    track_count,
                    config,
                    midi_hash,
                },
            );
        }
    }

    /// 发送编辑后的临时脏区域覆层显示命令（切换音轨前立即触发）
    #[allow(clippy::too_many_arguments)]
    pub fn send_hires_dirty_overlay(
        &mut self,
        track_idx: u16,
        group_notes: Vec<Vec<lumino_gfx::OnionSkinNote>>,
        ppq: u16,
        key_count: u16,
        total_ticks: u32,
        track_count: u16,
        config: lumino_gfx::HiResConfig,
        midi_hash: String,
    ) {
        if let Some(ref thread) = self.render_ctx.wgpu_render_thread {
            thread.send_control(
                lumino_gfx::render_thread::ControlCommand::ShowHiResDirtyOverlay {
                    track_idx,
                    group_notes,
                    ppq,
                    key_count,
                    total_ticks,
                    track_count,
                    config,
                    midi_hash,
                },
            );
        }
    }

    /// 获取高精度贴图生成时的 MIDI 哈希（供 runner 冷静期检查使用）
    pub fn hires_midi_hash(&self) -> Option<&str> {
        self.hires_midi_hash.as_deref()
    }

    /// 获取高精度贴图生成时的 (ppq, key_count, total_ticks)（供 runner 冷静期检查使用）
    pub fn hires_gen_info(&self) -> Option<(u16, u16, u32)> {
        self.hires_gen_info
    }

    /// 标记当前音轨高精度贴图为脏（音符编辑后调用）
    ///
    /// 同时收集该音轨的脏区域音符快照，用于生成临时贴图覆层。
    pub fn mark_hires_dirty(&mut self, track_idx: u16) {
        self.hires_dirty_tracks.insert(track_idx);
        // 收集当前音轨的所有音符作为脏区域快照
        let notes = self.get_track_notes_for_hires(track_idx);
        tracing::info!(
            "[onion-dirty] mark_hires_dirty: track={}, notes={}",
            track_idx,
            notes.len()
        );
        self.hires_dirty_regions.insert(track_idx, notes);
        self.hires_last_edit = Some(Instant::now());
    }

    /// 检查冷静期是否到期，返回需要重生成的脏音轨列表
    pub fn check_hires_regen(&mut self) -> Option<Vec<u16>> {
        if self.hires_dirty_tracks.is_empty() {
            return None;
        }
        let cooldown = self
            .hires_config
            .as_ref()
            .map(|c| c.cooldown_secs)
            .unwrap_or(10);
        if let Some(last) = self.hires_last_edit
            && last.elapsed().as_secs() >= cooldown
        {
            let dirty: Vec<u16> = self.hires_dirty_tracks.iter().copied().collect();
            self.hires_dirty_tracks.clear();
            self.hires_dirty_regions.clear();
            self.hires_last_edit = None;
            return Some(dirty);
        }
        None
    }

    /// 设置高精度贴图冷静期秒数（从配置初始化）
    pub fn set_hires_cooldown(&mut self, secs: u64) {
        if let Some(ref mut cfg) = self.hires_config {
            cfg.cooldown_secs = secs;
        }
    }

    /// 获取高精度贴图配置引用（供 runner 构建重生成上下文时使用）
    pub fn hires_config_ref(&self) -> Option<&lumino_gfx::HiResConfig> {
        self.hires_config.as_ref()
    }

    /// 立即触发脏音轨重生成（绕过冷静期）
    ///
    /// 在以下场景调用：
    /// - 用户从脏音轨切换到其他音轨
    /// - 需要在渲染线程开始后台重生，生成的贴图通过流式通道传回 GPU 上传
    ///
    /// 仅在 `hires_dirty_tracks` 包含该音轨且配置信息完整时生效。
    /// 调用后会从脏集合中移除该音轨。
    ///
    /// 重生成以音轨组为单位，使用整个 track_group 的最新音符数据，
    /// 避免同组其他音轨被覆盖为旧数据或空数据。
    pub fn force_hires_regen(&mut self, track_idx: u16) {
        tracing::info!("[onion-dirty] force_hires_regen 进入: track={}", track_idx);
        if !self.hires_dirty_tracks.remove(&track_idx) {
            tracing::info!(
                "[onion-dirty] force_hires_regen 退出: track={} 不在脏集合",
                track_idx
            );
            return; // 该音轨不脏，不触发
        }
        self.hires_dirty_regions.remove(&track_idx);

        let Some(cfg) = self.hires_config.clone() else {
            tracing::warn!("[onion-dirty] force_hires_regen 退出: hires_config 缺失");
            return;
        };
        let Some(hash) = self.hires_midi_hash.clone() else {
            tracing::warn!("[onion-dirty] force_hires_regen 退出: hires_midi_hash 缺失");
            return;
        };
        let Some((ppq, key_count, total_ticks)) = self.hires_gen_info else {
            tracing::warn!("[onion-dirty] force_hires_regen 退出: hires_gen_info 缺失");
            return;
        };

        // 音轨总数：取当前侧边栏音轨数与脏音轨索引+1 的较大值，
        // 确保干净启动时也能正确推断音轨组范围。
        let track_count = (self.root.sidebar.tracks.len() as u16).max(track_idx + 1);
        let group_notes = self.collect_group_notes(track_idx, track_count);
        tracing::info!(
            "[onion-dirty] force_hires_regen 发送命令: track={}, group_tracks={}, track_count={}, ppq={}, total_ticks={}",
            track_idx,
            group_notes.len(),
            track_count,
            ppq,
            total_ticks
        );

        self.send_hires_regen(
            track_idx,
            group_notes,
            ppq,
            key_count,
            total_ticks,
            track_count,
            cfg,
            hash,
        );
    }

    /// 收集指定音轨所在 track_group 的所有音轨音符
    ///
    /// 返回的 Vec 索引 0 对应该 track_group 的第一个音轨。
    fn collect_group_notes(
        &self,
        track_idx: u16,
        track_count: u16,
    ) -> Vec<Vec<lumino_gfx::OnionSkinNote>> {
        let track_group = (track_idx / lumino_gfx::TRACKS_PER_GROUP) as u32;
        let track_start = (track_group * lumino_gfx::TRACKS_PER_GROUP as u32) as u16;
        let track_end = (track_start + lumino_gfx::TRACKS_PER_GROUP).min(track_count);
        (track_start..track_end)
            .map(|t| self.get_track_notes_for_hires(t))
            .collect()
    }

    /// 获取指定音轨的音符列表（用于高精度贴图重生成）
    ///
    /// 当前音轨从 editor.notes 取，其他音轨从 track_notes 缓存取。
    pub fn get_track_notes_for_hires(&self, track_idx: u16) -> Vec<lumino_gfx::OnionSkinNote> {
        let editor = &self.root.editor;
        let current_track = editor.current_track();
        let notes = if current_track as u16 == track_idx {
            tracing::debug!(
                "[onion-dirty] get_track_notes_for_hires: track={} 使用当前编辑器音符",
                track_idx
            );
            &editor.editor_state.data.notes
        } else {
            tracing::debug!(
                "[onion-dirty] get_track_notes_for_hires: track={} 使用 track_notes 缓存",
                track_idx
            );
            match editor
                .editor_state
                .data
                .track_notes
                .get(&(track_idx as usize))
            {
                Some(n) => n,
                None => {
                    tracing::debug!(
                        "[onion-dirty] get_track_notes_for_hires: track={} 缓存未命中，返回空",
                        track_idx
                    );
                    return Vec::new();
                }
            }
        };
        let result: Vec<_> = notes
            .iter()
            .map(|n| {
                lumino_gfx::OnionSkinNote::from_ms(
                    n.tick,
                    n.tick + n.length,
                    n.key as u8,
                    onion_track_color(track_idx as usize),
                )
            })
            .collect();
        tracing::debug!(
            "[onion-dirty] get_track_notes_for_hires: track={}, count={}",
            track_idx,
            result.len()
        );
        result
    }

    /// 取出洋葱皮生成进度（runner 每帧调用并转发到进度窗口）
    pub fn drain_onion_progress(&self) -> Vec<(String, f32)> {
        self.render_ctx
            .wgpu_render_thread
            .as_ref()
            .map(|t| t.drain_onion_progress())
            .unwrap_or_default()
    }

    /// 获取 root 引用
    pub fn root(&self) -> &root::Root {
        &self.root
    }

    /// 获取 root 可变引用
    pub fn root_mut(&mut self) -> &mut root::Root {
        &mut self.root
    }

    /// 获取当前侧边栏音轨数量（用于推断高精度贴图音轨组范围）
    pub fn track_count(&self) -> usize {
        self.root.sidebar.tracks.len()
    }

    /// 获取设置面板引用
    pub fn settings(&self) -> &settings::SettingsPanel {
        self.root.settings()
    }

    /// 调整窗口大小
    pub fn resize(&mut self, width: u32, height: u32) {
        self.render_ctx.viewport = Viewport::with_physical_size(
            Size::new(width, height),
            self.window_ctx.window.scale_factor() as f32,
        );
    }

    /// 获取当前光标位置（逻辑坐标）
    pub fn cursor_position(&self) -> Option<iced_core::Point> {
        self.window_ctx.cursor_position
    }

    /// 收集所有组件的内存占用快照（Root + RenderCache）
    pub fn memory_breakdown(&self) -> root::MemoryBreakdown {
        let mut breakdown = self.root.memory_breakdown();

        // 从 RenderCache 获取主音符双缓冲容量
        let (front_cap, front_len) = self
            .render_ctx
            .render_cache
            .note_instances_buffer
            .front_info();
        let (back_cap, back_len) = self
            .render_ctx
            .render_cache
            .note_instances_buffer
            .back_info();
        // 将双缓冲容量写入 breakdown 的附加字段
        breakdown.note_instances_front_cap = front_cap;
        breakdown.note_instances_front_len = front_len;
        breakdown.note_instances_back_cap = back_cap;
        breakdown.note_instances_back_len = back_len;
        breakdown.note_instance_size = std::mem::size_of::<lumino_gfx::NoteInstance>() as usize;

        breakdown
    }
}

/// 洋葱皮音轨调色板（按音轨索引循环取色，alpha 固定 255）
fn onion_track_color(track_idx: usize) -> [u8; 4] {
    const PALETTE: [[u8; 4]; 8] = [
        [200, 80, 80, 255],
        [80, 200, 120, 255],
        [80, 120, 220, 255],
        [220, 200, 80, 255],
        [200, 100, 200, 255],
        [80, 200, 200, 255],
        [240, 150, 80, 255],
        [180, 180, 180, 255],
    ];
    PALETTE[track_idx % PALETTE.len()]
}

/// 字体名称缓存 —— OnceLock 确保只泄漏一次，而不是每次重绘都泄漏
static FONT_NAME_CACHE: OnceLock<String> = OnceLock::new();

/// 根据配置创建字体
///
/// 使用系统字体名称或默认字体
fn create_font_from_config(ui_config: &config::UiConfig) -> Font {
    // 优先使用自定义字体路径
    if !ui_config.program_font_path.is_empty() {
        let path = std::path::Path::new(&ui_config.program_font_path);
        if path.exists() {
            tracing::info!("检测到自定义字体路径: {:?}", path);
            // 自定义字体文件加载需要重启应用才能生效
            // 这里只记录日志
        }
    }

    // 其次使用系统字体名称
    if !ui_config.program_font_name.is_empty() {
        let cached = FONT_NAME_CACHE.get_or_init(|| ui_config.program_font_name.clone());

        tracing::info!("应用字体: {}", cached);
        return Font::with_name(cached.as_str());
    }

    // 使用默认字体
    tracing::info!("使用默认字体 (SansSerif)");
    Font::default()
}
