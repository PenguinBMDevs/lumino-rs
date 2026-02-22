pub mod state;
pub mod grid;
pub mod note;
pub mod scrollbar;
pub mod scrollbar_widget;

use crate::{Element, Message};
use iced_widget::canvas::{self, Canvas};
use iced_core::{Length, Point};
use lumino_gfx::NoteInstance;

pub use state::ViewState;
pub use grid::PianoRollGrid;
use note::Note;

/// 钢琴卷帘编辑器
pub struct Editor {
    pub state: ViewState,
    pub grid_cache: canvas::Cache<crate::Renderer>,
    pub max_scroll_x: f32,
    pub max_scroll_y: f32,
    /// 当前鼠标在窗口中的位置
    pub cursor_position: Option<Point>,
    /// Canvas 在窗口中的偏移量（用于坐标转换）
    pub canvas_offset: Point,
    /// Canvas 尺寸（宽, 高）
    pub canvas_size: Point,
}

impl Editor {
    pub fn new() -> Self {
        let mut editor = Self {
            state: ViewState::default(),
            grid_cache: canvas::Cache::new(),
            max_scroll_x: 0.0,
            max_scroll_y: 0.0,
            cursor_position: None,
            canvas_offset: Point::new(0.0, 0.0),
            canvas_size: Point::new(0.0, 0.0),
        };
        editor.max_scroll_x = editor.state.total_ticks as f32 * editor.state.zoom_x;
        editor.max_scroll_y = editor.state.visible_key_count as f32 * editor.state.zoom_y;
        editor
    }

    /// 构建编辑器视图
    pub fn view(
        &self,
        on_scroll_x: impl Fn(f32) -> Message + 'static,
        on_scroll_y: impl Fn(f32) -> Message + 'static,
        on_zoom_x: impl Fn(f32, f32) -> Message + 'static,
        on_zoom_y: impl Fn(f32, f32) -> Message + 'static,
    ) -> Element<'_> {
        // 创建带鼠标追踪的 Canvas
        let grid = Canvas::new(PianoRollGrid::new(&self.state, &self.grid_cache))
            .width(Length::Fill)
            .height(Length::Fill);

        let horizontal_scrollbar = scrollbar_widget::ScrollbarWidget::horizontal(
            self.state.scroll_x,
            self.max_scroll_x,
            self.state.zoom_x,
            on_scroll_x,
            on_zoom_x,
        );

        let vertical_scrollbar = scrollbar_widget::ScrollbarWidget::vertical(
            self.state.scroll_y,
            self.max_scroll_y,
            self.state.zoom_y,
            on_scroll_y,
            on_zoom_y,
        );

        let content_with_vscroll = iced_widget::row![grid, vertical_scrollbar];

        iced_widget::column![content_with_vscroll, horizontal_scrollbar].into()
    }

    /// 获取当前需要绘制的音符实例（用于 wgpu 渲染）
    /// 
    /// 目前只返回鼠标位置的预览音符，后续可扩展为返回所有 MIDI 音符
    /// 音符只在 Canvas 区域内显示
    pub fn get_note_instances(&self, theme: &crate::Theme) -> Vec<NoteInstance> {
        let mut instances = Vec::new();
        
        // 如果有鼠标位置，检查是否在 Canvas 区域内
        if let Some(pos) = self.cursor_position {
            // 计算 Canvas 局部坐标
            let local_pos = Point::new(
                pos.x - self.canvas_offset.x,
                pos.y - self.canvas_offset.y,
            );
            
            // 检查鼠标是否在 Canvas 有效区域内（考虑键盘宽度）
            if !self.is_inside_canvas(local_pos) {
                return instances; // 在区域外，不显示音符
            }
            
            // 计算音符（基于 Canvas 局部坐标）
            let note = Note::from_mouse_position(local_pos, &self.state, theme);
            
            // 转换为窗口坐标：加上 Canvas 偏移
            let window_pos = Point::new(
                note.position.x + self.canvas_offset.x,
                note.position.y + self.canvas_offset.y,
            );
            
            instances.push(NoteInstance::new(
                window_pos.x,
                window_pos.y,
                note.size.x,
                note.size.y,
                [note.color.r, note.color.g, note.color.b, note.color.a],
            ));
        }
        
        // TODO: 在这里添加实际的 MIDI 音符实例
        
        instances
    }
    
    /// 检查点是否在 Canvas 有效区域内
    /// 有效区域 = Canvas 区域减去键盘区域（左侧）和滚动条区域（底部和右侧）
    /// 同时避开顶部可能被下拉菜单覆盖的区域
    fn is_inside_canvas(&self, local_pos: Point) -> bool {
        // 基本的 Canvas 边界检查
        if local_pos.x < 0.0 || local_pos.x > self.canvas_size.x {
            return false;
        }
        if local_pos.y < 0.0 || local_pos.y > self.canvas_size.y {
            return false;
        }
        
        // 检查是否在键盘区域外（x 必须大于键盘宽度）
        if local_pos.x < self.state.keyboard_width {
            return false;
        }
        
        // 避开顶部区域（防止与下拉菜单重叠）
        // 顶部 40 像素区域不渲染音符（给下拉菜单留空间）
        const MENU_SAFE_ZONE: f32 = 40.0;
        if local_pos.y < MENU_SAFE_ZONE {
            return false;
        }
        
        true
    }

    /// 更新鼠标位置（由外部调用）
    pub fn update_cursor_position(&mut self, position: Option<Point>) {
        self.cursor_position = position;
    }

    /// 更新 Canvas 偏移量（用于坐标转换）
    pub fn set_canvas_offset(&mut self, offset: Point) {
        self.canvas_offset = offset;
    }
    
    /// 更新 Canvas 尺寸
    pub fn set_canvas_size(&mut self, size: Point) {
        self.canvas_size = size;
    }

    // ========== 滚动控制 ==========
    
    pub fn set_max_scroll_x(&mut self, max_scroll: f32) {
        self.max_scroll_x = max_scroll;
    }

    pub fn set_max_scroll_y(&mut self, max_scroll: f32) {
        self.max_scroll_y = max_scroll;
    }

    pub fn scroll_x(&self) -> f32 {
        self.state.scroll_x
    }

    pub fn scroll_y(&self) -> f32 {
        self.state.scroll_y
    }

    pub fn set_scroll_x(&mut self, scroll_x: f32) {
        self.state.scroll_x = scroll_x.max(0.0).min(self.max_scroll_x);
        self.grid_cache.clear();
    }

    pub fn set_scroll_y(&mut self, scroll_y: f32) {
        self.state.scroll_y = scroll_y.max(0.0).min(self.max_scroll_y);
        self.grid_cache.clear();
    }

    pub fn set_zoom_x(&mut self, zoom_x: f32, fixed_ratio: f32) {
        let old_zoom_x = self.state.zoom_x;
        self.state.zoom_x = zoom_x.max(0.001).min(10.0);
        
        let ratio = self.state.zoom_x / old_zoom_x;
        let view_width = (self.canvas_size.x - self.state.keyboard_width).max(0.0);
        
        // 保持固定比例处的 tick 不变
        let fixed_pixel = self.state.scroll_x + view_width * fixed_ratio;
        self.state.scroll_x = fixed_pixel * ratio - view_width * fixed_ratio;
        
        self.max_scroll_x = self.state.total_ticks as f32 * self.state.zoom_x;
        self.state.scroll_x = self.state.scroll_x.max(0.0).min(self.max_scroll_x);
        self.grid_cache.clear();
    }

    pub fn set_zoom_y(&mut self, zoom_y: f32, fixed_ratio: f32) {
        let old_zoom_y = self.state.zoom_y;
        self.state.zoom_y = zoom_y.max(5.0).min(100.0);
        
        let ratio = self.state.zoom_y / old_zoom_y;
        let view_height = self.canvas_size.y.max(0.0);
        
        // 保持固定比例处的 key 不变
        let fixed_pixel = self.state.scroll_y + view_height * fixed_ratio;
        self.state.scroll_y = fixed_pixel * ratio - view_height * fixed_ratio;
        
        self.max_scroll_y = self.state.visible_key_count as f32 * self.state.zoom_y;
        self.state.scroll_y = self.state.scroll_y.max(0.0).min(self.max_scroll_y);
        self.grid_cache.clear();
    }

    // ========== 键盘设置 ==========

    pub fn set_visible_key_count(&mut self, count: u16) {
        let clamped_count = count.clamp(1, 256);
        self.state.visible_key_count = clamped_count;
        self.max_scroll_y = clamped_count as f32 * self.state.zoom_y;
        if self.state.scroll_y > self.max_scroll_y {
            self.state.scroll_y = self.max_scroll_y;
        }
        self.grid_cache.clear();
    }

    pub fn visible_key_count(&self) -> u16 {
        self.state.visible_key_count
    }

    pub fn set_keyboard_width(&mut self, width: f32) {
        self.state.keyboard_width = width.max(0.0);
        self.grid_cache.clear();
    }

    pub fn keyboard_width(&self) -> f32 {
        self.state.keyboard_width
    }

    // ========== 音符设置 ==========

    pub fn set_snap_precision(&mut self, precision: f32) {
        self.state.snap_precision = precision.max(1.0);
        self.grid_cache.clear();
    }

    pub fn snap_precision(&self) -> f32 {
        self.state.snap_precision
    }

    pub fn set_default_note_length(&mut self, length: f32) {
        self.state.default_note_length = length.max(1.0);
        self.grid_cache.clear();
    }

    pub fn default_note_length(&self) -> f32 {
        self.state.default_note_length
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}
