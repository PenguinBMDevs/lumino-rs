pub mod state;
pub mod grid;
pub mod note;
pub mod scrollbar;
pub mod scrollbar_widget;

use crate::{Element, Message, message::{EditorAction, AudioAction}};
use iced_widget::canvas::{self, Canvas};
use iced_core::{Length, Point};
use lumino_gfx::NoteInstance;

pub use state::ViewState;
pub use grid::PianoRollGrid;
use note::Note;

#[derive(Debug, Clone, Default)]
pub enum EditState {
    #[default]
    Idle,
    Drawing { start_tick: f32, key: u16, current_tick: f32 },
    Dragging { note_index: usize, offset_tick: f32, offset_key: i32 },
    ResizingStart { note_index: usize, original_tick: f32, original_length: f32 },
    ResizingEnd { note_index: usize },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HitType {
    Start,
    Middle,
    End,
}

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
    
    pub notes: Vec<Note>,
    pub edit_state: EditState,
    pub hover_state: Option<(usize, HitType)>,
    pub pending_audio_actions: Vec<AudioAction>,
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
            notes: Vec::new(),
            edit_state: EditState::Idle,
            hover_state: None,
            pending_audio_actions: Vec::new(),
        };
        editor.max_scroll_x = editor.state.total_ticks as f32 * editor.state.zoom_x;
        editor.max_scroll_y = editor.state.visible_key_count as f32 * editor.state.zoom_y;
        editor
    }

    pub fn handle_action(&mut self, action: EditorAction) {
        // 清空上一次的待处理音频动作
        self.pending_audio_actions.clear();
        
        match action {
            EditorAction::Pressed(pos) => {
                if !self.is_inside_canvas(pos) {
                    return;
                }
                let tick = self.x_to_tick(pos.x);
                let key = self.y_to_key(pos.y);
                let snapped_tick = self.snap_tick(tick);

                if let Some((index, hit_type)) = self.hit_test_note(pos) {
                    match hit_type {
                        HitType::Start => {
                            let note = &self.notes[index];
                            self.edit_state = EditState::ResizingStart {
                                note_index: index,
                                original_tick: note.tick,
                                original_length: note.length,
                            };
                        }
                        HitType::End => {
                            self.edit_state = EditState::ResizingEnd {
                                note_index: index,
                            };
                        }
                        HitType::Middle => {
                            let note = &self.notes[index];
                            self.edit_state = EditState::Dragging {
                                note_index: index,
                                offset_tick: tick - note.tick,
                                offset_key: key as i32 - note.key as i32,
                            };
                        }
                    }
                } else {
                    self.edit_state = EditState::Drawing {
                        start_tick: snapped_tick,
                        key,
                        current_tick: snapped_tick,
                    };
                    // 播放音符音频（按下时发声）
                    self.pending_audio_actions.push(AudioAction::PlayNote {
                        key: key as u8,
                        velocity: 100, // 使用固定力度
                    });
                }
            }
            EditorAction::Moved(pos) => {
                let tick = self.x_to_tick(pos.x);
                let key = self.y_to_key(pos.y);
                let snapped_tick = self.snap_tick(tick);

                self.hover_state = self.hit_test_note(pos);

                let mut new_tick_val = None;
                let mut new_key_val = None;
                let mut new_length_val = None;

                let snap_precision = self.state.snap_precision;
                let visible_key_count = self.state.visible_key_count;

                match &mut self.edit_state {
                    EditState::Drawing { current_tick, .. } => {
                        *current_tick = snapped_tick;
                    }
                    EditState::Dragging { offset_tick, offset_key, .. } => {
                        let new_tick = ((tick - *offset_tick) / snap_precision).round() * snap_precision;
                        new_tick_val = Some(new_tick.max(0.0));
                        new_key_val = Some((key as i32 - *offset_key).clamp(0, visible_key_count as i32 - 1) as u16);
                    }
                    EditState::ResizingStart { original_tick, original_length, .. } => {
                        let end_tick = *original_tick + *original_length;
                        let new_tick = snapped_tick.min(end_tick - snap_precision).max(0.0);
                        new_tick_val = Some(new_tick);
                        new_length_val = Some(end_tick - new_tick);
                    }
                    EditState::ResizingEnd { note_index, .. } => {
                        if let Some(note) = self.notes.get(*note_index) {
                            new_length_val = Some((snapped_tick - note.tick).max(snap_precision));
                        }
                    }
                    EditState::Idle => {}
                }

                match self.edit_state {
                    EditState::Dragging { note_index, .. } |
                    EditState::ResizingStart { note_index, .. } |
                    EditState::ResizingEnd { note_index, .. } => {
                        if let Some(note) = self.notes.get_mut(note_index) {
                            if let Some(t) = new_tick_val { note.tick = t; }
                            if let Some(k) = new_key_val { note.key = k; }
                            if let Some(l) = new_length_val { note.length = l; }
                        }
                    }
                    _ => {}
                }
            }
            EditorAction::Released => {
                match self.edit_state {
                    EditState::Drawing { start_tick, key, current_tick } => {
                        let (tick, length) = if current_tick > start_tick {
                            (start_tick, current_tick - start_tick)
                        } else if current_tick < start_tick {
                            (current_tick, start_tick - current_tick)
                        } else {
                            (start_tick, self.state.default_note_length)
                        };
                        let length = length.max(self.state.snap_precision);
                        self.notes.push(Note::new(tick, key, length));
                    }
                    _ => {}
                }
                self.edit_state = EditState::Idle;
            }
        }
    }

    fn x_to_tick(&self, x: f32) -> f32 {
        (x - self.state.keyboard_width + self.state.scroll_x) / self.state.zoom_x
    }

    fn y_to_key(&self, y: f32) -> u16 {
        let max_key_index = (self.state.visible_key_count - 1) as f32;
        let key_f32 = max_key_index - (y + self.state.scroll_y) / self.state.zoom_y;
        key_f32.round().clamp(0.0, max_key_index) as u16
    }

    fn snap_tick(&self, tick: f32) -> f32 {
        (tick / self.state.snap_precision).round() * self.state.snap_precision
    }

    pub fn hit_test_note(&self, pos: Point) -> Option<(usize, HitType)> {
        let tick = self.x_to_tick(pos.x);
        let key = self.y_to_key(pos.y);
        
        for (i, note) in self.notes.iter().enumerate().rev() {
            if note.key == key && tick >= note.tick && tick <= note.tick + note.length {
                let start_dist = (tick - note.tick).abs();
                let end_dist = (tick - (note.tick + note.length)).abs();
                let edge_threshold = 10.0 / self.state.zoom_x;

                if end_dist < edge_threshold {
                    return Some((i, HitType::End));
                } else if start_dist < edge_threshold {
                    return Some((i, HitType::Start));
                } else {
                    return Some((i, HitType::Middle));
                }
            }
        }
        None
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
        let grid = Canvas::new(PianoRollGrid::new(self))
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
    pub fn get_note_instances(&self, theme: &crate::Theme, _sidebar_width: f32) -> Vec<NoteInstance> {
        let mut instances = Vec::new();
        let palette = theme.extended_palette();
        
        // 默认音符颜色（更弱颜色）
        let default_color = palette.primary.weak.color;
        // 悬停音符颜色
        let hover_color = palette.primary.base.color;
        // 正在绘制/选中的音符颜色（最强颜色）
        let active_color = palette.primary.strong.color;

        // 渲染已放置的音符
        for (i, note) in self.notes.iter().enumerate() {
            let color = match self.edit_state {
                EditState::Dragging { note_index, .. } |
                EditState::ResizingStart { note_index, .. } |
                EditState::ResizingEnd { note_index, .. } if note_index == i => active_color,
                EditState::Idle if self.hover_state.map_or(false, |(idx, _)| idx == i) => hover_color,
                _ => default_color,
            };
            
            let mut instance = note.to_instance(&self.state, color);
            // 转换为窗口坐标：加上 Canvas 偏移
            instance.position[0] += self.canvas_offset.x;
            instance.position[1] += self.canvas_offset.y;
            instances.push(instance);
        }

        // 渲染正在绘制的音符
        if let EditState::Drawing { start_tick, key, current_tick } = self.edit_state {
            let (tick, length) = if current_tick > start_tick {
                (start_tick, current_tick - start_tick)
            } else if current_tick < start_tick {
                (current_tick, start_tick - current_tick)
            } else {
                (start_tick, self.state.default_note_length)
            };
            let length = length.max(self.state.snap_precision);
            let drawing_note = Note::new(tick, key, length);
            
            let mut instance = drawing_note.to_instance(&self.state, active_color);
            instance.position[0] += self.canvas_offset.x;
            instance.position[1] += self.canvas_offset.y;
            instances.push(instance);
        } else if let Some(pos) = self.cursor_position {
            // 预览音符 - 仅在没有悬停在其他音符上时显示
            if self.hover_state.is_none() {
                let local_pos = Point::new(
                    pos.x - self.canvas_offset.x,
                    pos.y - self.canvas_offset.y,
                );
                if self.is_inside_canvas(local_pos) {
                    let tick = self.snap_tick(self.x_to_tick(local_pos.x));
                    let key = self.y_to_key(local_pos.y);
                    let preview_note = Note::new(tick, key, self.state.default_note_length);
                    
                    let mut preview_color = default_color;
                    preview_color.a = 0.5;
                    
                    let mut instance = preview_note.to_instance(&self.state, preview_color);
                    instance.position[0] += self.canvas_offset.x;
                    instance.position[1] += self.canvas_offset.y;
                    instances.push(instance);
                }
            }
        }

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
    
    /// 获取并清空待处理的音频动作
    pub fn take_audio_actions(&mut self) -> Vec<AudioAction> {
        std::mem::take(&mut self.pending_audio_actions)
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
