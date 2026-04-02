//! Host 模块 - UI 宿主，管理渲染和事件处理
//!
//! 该模块已拆分为以下子模块：
//! - `types`: 类型定义和工具函数
//! - `render`: 渲染逻辑（iced UI 和 wgpu 音符）
//! - `event`: 事件处理（窗口事件、输入）
//! - `editor_ops`: 编辑器操作（音符、洋葱皮）
//! - `dialog`: 对话框和协作功能

use std::{sync::Arc, time::Instant};

use iced_wgpu::{Engine, Renderer, graphics::Viewport};
use lumino_gfx::NoteRenderer;

use iced_winit::runtime::user_interface::Cache;
use iced_winit::{Clipboard, winit};

use iced_core::{Font, Pixels, Size, mouse};

use crate::{config, root, settings, window};

mod dialog;
mod editor_ops;
mod event;
mod render;
pub mod types;

pub use types::{DialogResult, NoteData, TrackNotes};

/// UI 宿主 - 管理 iced 渲染和 wgpu 音符渲染
pub struct Host {
    pub(crate) window: Arc<winit::window::Window>,
    pub(crate) root: root::Root,
    pub(crate) renderer: Renderer,
    pub(crate) events: Vec<iced_core::Event>,
    pub(crate) cursor: mouse::Cursor,
    pub(crate) cache: Cache,
    pub(crate) clipboard: Clipboard,
    pub(crate) viewport: Viewport,
    pub(crate) pending_window_action: Option<window::TrafficAction>,
    pub(crate) pending_drag: bool,
    /// 逻辑光标位置（用于音符预览和触控拖动）
    pub cursor_position: Option<iced_core::Point>,
    pub(crate) last_fps_update: Instant,
    /// 帧计数器（用于 FPS 计算）
    pub(crate) frame_count: u32,
    /// 是否正在拖拽调整工具栏高度
    pub(crate) is_toolbar_resizing: bool,
    /// 音符渲染器
    pub(crate) note_renderer: NoteRenderer,
    /// 上一帧时间
    pub(crate) last_frame_time: Instant,
}

impl Host {
    /// 创建新的 Host
    pub fn new(
        window: Arc<winit::window::Window>,
        width: u32,
        height: u32,
        ui_config: &config::UiConfig,
        gfx: &lumino_gfx::Context,
        is_progress: bool,
    ) -> Self {
        let viewport =
            Viewport::with_physical_size(Size::new(width, height), window.scale_factor() as f32);

        let clipboard = Clipboard::connect(window.clone());

        // 初始化 iced 渲染器
        let renderer = {
            let engine = Engine::new(
                &gfx.adapter,
                gfx.device.clone(),
                gfx.queue.clone(),
                gfx.format,
                None,
                iced_wgpu::graphics::Shell::headless(),
            );
            Renderer::new(engine, Font::default(), Pixels::from(16))
        };

        // 创建 wgpu 音符渲染器
        let note_renderer = NoteRenderer::new(&gfx.device, gfx.format);

        Self {
            window,
            root: if is_progress {
                root::Root::new_progress(&ui_config.theme)
            } else {
                root::Root::new(ui_config)
            },
            renderer,
            events: Vec::new(),
            cursor: mouse::Cursor::Unavailable,
            cache: Cache::new(),
            clipboard,
            viewport,
            pending_window_action: None,
            pending_drag: false,
            note_renderer,
            cursor_position: None,
            last_frame_time: Instant::now(),
            last_fps_update: Instant::now(),
            frame_count: 0,
            is_toolbar_resizing: false,
        }
    }

    /// 创建对话框 Host
    pub fn new_dialog(
        window: Arc<winit::window::Window>,
        width: u32,
        height: u32,
        ui_config: &config::UiConfig,
        gfx: &lumino_gfx::Context,
    ) -> Self {
        let viewport =
            Viewport::with_physical_size(Size::new(width, height), window.scale_factor() as f32);

        let clipboard = Clipboard::connect(window.clone());

        // 初始化 iced 渲染器
        let renderer = {
            let engine = Engine::new(
                &gfx.adapter,
                gfx.device.clone(),
                gfx.queue.clone(),
                gfx.format,
                None,
                iced_wgpu::graphics::Shell::headless(),
            );
            Renderer::new(engine, Font::default(), Pixels::from(16))
        };

        // 创建 wgpu 音符渲染器
        let note_renderer = NoteRenderer::new(&gfx.device, gfx.format);

        Self {
            window,
            root: root::Root::new_dialog(&ui_config.theme),
            renderer,
            events: Vec::new(),
            cursor: mouse::Cursor::Unavailable,
            cache: Cache::new(),
            clipboard,
            viewport,
            pending_window_action: None,
            pending_drag: false,
            note_renderer,
            cursor_position: None,
            last_frame_time: Instant::now(),
            last_fps_update: Instant::now(),
            frame_count: 0,
            is_toolbar_resizing: false,
        }
    }

    /// 获取 root 引用
    pub fn root(&self) -> &root::Root {
        &self.root
    }

    /// 获取 root 可变引用
    pub fn root_mut(&mut self) -> &mut root::Root {
        &mut self.root
    }

    /// 获取设置面板引用
    pub fn settings(&self) -> &settings::SettingsPanel {
        self.root.settings()
    }

    /// 调整窗口大小
    pub fn resize(&mut self, width: u32, height: u32) {
        self.viewport = Viewport::with_physical_size(
            Size::new(width, height),
            self.window.scale_factor() as f32,
        );
    }

    /// 获取当前光标位置（逻辑坐标）
    pub fn cursor_position(&self) -> Option<iced_core::Point> {
        self.cursor_position
    }
}
