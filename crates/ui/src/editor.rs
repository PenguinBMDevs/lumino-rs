pub mod grid;
pub mod note;
pub mod onion_skin;
pub mod scrollbar;
pub mod scrollbar_widget;
pub mod state;

use crate::{
    Element, Message,
    message::{AudioAction, EditorAction},
};
use iced_core::{Length, Point};
use iced_widget::canvas::{self, Canvas};
use lumino_gfx::NoteInstance;

pub use grid::PianoRollGrid;
use note::Note;
pub use onion_skin::OnionSkinConfig;
pub use state::ViewState;

#[derive(Debug, Clone, Default)]
pub enum EditState {
    #[default]
    Idle,
    Drawing {
        start_tick: f32,
        key: u16,
        current_tick: f32,
    },
    /// 预备拖动状态：点击音符后等待判断是点击还是拖动
    PendingDrag {
        note_index: usize,
        start_pos: Point,
        original_tick: f32,
        original_key: u16,
    },
    Dragging {
        note_index: usize,
        offset_tick: f32,
        offset_key: i32,
        last_played_key: u16, // 上一次播放的音高，用于避免重复播放
    },
    ResizingStart {
        note_index: usize,
        original_tick: f32,
        original_length: f32,
    },
    ResizingEnd {
        note_index: usize,
    },
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

    /// 当前编辑的音轨索引
    pub current_track: usize,
    /// 按音轨存储的音符（用于无 MIDI 文件时的多音轨编辑）
    pub track_notes: std::collections::HashMap<usize, Vec<Note>>,

    /// 洋葱皮配置
    onion_skin_config: OnionSkinConfig,
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
            current_track: 0,
            track_notes: std::collections::HashMap::new(),
            onion_skin_config: OnionSkinConfig::new(),
        };
        editor.max_scroll_x = editor.state.total_ticks as f32 * editor.state.zoom_x;
        editor.max_scroll_y = editor.state.visible_key_count as f32 * editor.state.zoom_y;
        editor
    }

    /// 主入口：处理编辑器动作
    pub fn handle_action(&mut self, action: EditorAction) {
        self.pending_audio_actions.clear();

        match action {
            EditorAction::Pressed(pos) => self.handle_pressed(pos),
            EditorAction::Moved(pos) => self.handle_moved(pos),
            EditorAction::Released => self.handle_released(),
            EditorAction::Scrolled { delta_x, delta_y } => self.handle_scrolled(delta_x, delta_y),
            EditorAction::DoubleClicked(pos) => self.handle_double_clicked(pos),
            EditorAction::DeletePressed => self.handle_delete_pressed(),
        }
    }

    /// 处理鼠标按下事件
    fn handle_pressed(&mut self, pos: iced_core::Point) {
        if !self.is_inside_canvas(pos) {
            return;
        }

        let tick = self.x_to_tick(pos.x);
        let key = self.y_to_key(pos.y);
        let snapped_tick = self.snap_tick(tick);

        if let Some((index, hit_type)) = self.hit_test_note(pos) {
            self.start_note_edit(index, hit_type, pos);
        } else {
            self.start_drawing(snapped_tick, key);
        }
    }

    /// 开始编辑现有音符
    fn start_note_edit(&mut self, index: usize, hit_type: HitType, pos: iced_core::Point) {
        let note = &self.notes[index];

        match hit_type {
            HitType::Start => {
                self.edit_state = EditState::ResizingStart {
                    note_index: index,
                    original_tick: note.tick,
                    original_length: note.length,
                };
            }
            HitType::End => {
                self.edit_state = EditState::ResizingEnd { note_index: index };
            }
            HitType::Middle => {
                self.edit_state = EditState::PendingDrag {
                    note_index: index,
                    start_pos: pos,
                    original_tick: note.tick,
                    original_key: note.key,
                };
                self.play_note_audio(note.key, "点击音符");
            }
        }
    }

    /// 开始绘制新音符
    fn start_drawing(&mut self, snapped_tick: f32, key: u16) {
        self.edit_state = EditState::Drawing {
            start_tick: snapped_tick,
            key,
            current_tick: snapped_tick,
        };
        self.play_note_audio(key, "新音符");
    }

    /// 播放音符音频
    fn play_note_audio(&mut self, key: u16, context: &str) {
        tracing::debug!("Editor: 推送 PlayNote ({}) key={}", context, key);
        self.pending_audio_actions.push(AudioAction::PlayNote {
            key: key as u8,
            velocity: 100,
        });
    }

    /// 处理鼠标移动事件
    fn handle_moved(&mut self, pos: iced_core::Point) {
        let tick = self.x_to_tick(pos.x);
        let key = self.y_to_key(pos.y);
        let snapped_tick = self.snap_tick(tick);

        self.hover_state = self.hit_test_note(pos);

        let (new_tick, new_key, new_length) =
            self.calculate_edit_changes(pos, tick, key, snapped_tick);
        self.apply_note_changes(new_tick, new_key, new_length);
    }

    /// 计算编辑状态的变化值
    fn calculate_edit_changes(
        &mut self,
        pos: iced_core::Point,
        tick: f32,
        key: u16,
        snapped_tick: f32,
    ) -> (Option<f32>, Option<u16>, Option<f32>) {
        let mut new_tick = None;
        let mut new_key = None;
        let mut new_length = None;
        let mut note_to_play = None;

        let snap_precision = self.state.snap_precision;
        let visible_key_count = self.state.visible_key_count;

        // 先处理可能改变 edit_state 的情况
        if let EditState::PendingDrag {
            note_index,
            start_pos,
            original_tick,
            original_key,
        } = self.edit_state
        {
            if self.should_start_dragging(pos, start_pos) {
                let tick = self.x_to_tick(start_pos.x);
                let key = self.y_to_key(start_pos.y);
                self.edit_state = EditState::Dragging {
                    note_index,
                    offset_tick: tick - original_tick,
                    offset_key: key as i32 - original_key as i32,
                    last_played_key: original_key,
                };
            }
        }

        match &mut self.edit_state {
            EditState::Drawing { current_tick, .. } => {
                *current_tick = snapped_tick;
            }
            EditState::Dragging {
                offset_tick,
                offset_key,
                last_played_key,
                ..
            } => {
                let calculated_tick =
                    ((tick - *offset_tick) / snap_precision).round() * snap_precision;
                let calculated_key =
                    (key as i32 - *offset_key).clamp(0, visible_key_count as i32 - 1) as u16;
                new_key = Some(calculated_key);
                new_tick = Some(calculated_tick.max(0.0));

                if calculated_key != *last_played_key {
                    note_to_play = Some(calculated_key);
                    *last_played_key = calculated_key;
                }
            }
            EditState::ResizingStart {
                original_tick,
                original_length,
                ..
            } => {
                let end_tick = *original_tick + *original_length;
                let calculated_tick = snapped_tick.min(end_tick - snap_precision).max(0.0);
                new_tick = Some(calculated_tick);
                new_length = Some(end_tick - calculated_tick);
            }
            EditState::ResizingEnd { note_index, .. } => {
                if let Some(note) = self.notes.get(*note_index) {
                    new_length = Some((snapped_tick - note.tick).max(snap_precision));
                }
            }
            _ => {}
        }

        // 在 match 之后播放音频，避免借用冲突
        if let Some(k) = note_to_play {
            self.play_note_audio(k, "拖动变化");
        }

        (new_tick, new_key, new_length)
    }

    /// 检查是否应该开始拖动
    fn should_start_dragging(&self, pos: iced_core::Point, start_pos: iced_core::Point) -> bool {
        let delta_y = pos.y - start_pos.y;
        let key_threshold = self.state.zoom_y * 0.5;
        delta_y.abs() > key_threshold
    }

    /// 应用音符变化
    fn apply_note_changes(
        &mut self,
        new_tick: Option<f32>,
        new_key: Option<u16>,
        new_length: Option<f32>,
    ) {
        let note_index = match self.edit_state {
            EditState::Dragging { note_index, .. }
            | EditState::ResizingStart { note_index, .. }
            | EditState::ResizingEnd { note_index, .. } => note_index,
            _ => return,
        };

        if let Some(note) = self.notes.get_mut(note_index) {
            if let Some(t) = new_tick {
                note.tick = t;
            }
            if let Some(k) = new_key {
                note.key = k;
            }
            if let Some(l) = new_length {
                note.length = l;
            }
        }
    }

    /// 处理鼠标释放事件
    fn handle_released(&mut self) {
        match self.edit_state {
            EditState::Drawing {
                start_tick,
                key,
                current_tick,
            } => {
                self.finish_drawing(start_tick, key, current_tick);
            }
            EditState::PendingDrag { .. } => {
                // 只是点击，没有拖动，保持音符不变
            }
            _ => {}
        }
        self.edit_state = EditState::Idle;
    }

    /// 完成绘制新音符
    fn finish_drawing(&mut self, start_tick: f32, key: u16, current_tick: f32) {
        let (tick, length) = if current_tick > start_tick {
            (start_tick, current_tick - start_tick)
        } else if current_tick < start_tick {
            (current_tick, start_tick - current_tick)
        } else {
            (start_tick, self.state.default_note_length)
        };

        let length = length.max(self.state.snap_precision);
        self.notes.push(Note::new(tick, key, length));
        self.track_notes
            .insert(self.current_track, self.notes.clone());

        tracing::debug!(
            "Editor: saved {} notes to track {}",
            self.notes.len(),
            self.current_track
        );
    }

    /// 处理滚动事件
    fn handle_scrolled(&mut self, delta_x: f32, delta_y: f32) {
        let new_scroll_y = self.state.scroll_y - delta_y;
        self.set_scroll_y(new_scroll_y);

        if delta_x != 0.0 {
            let new_scroll_x = self.state.scroll_x - delta_x;
            self.set_scroll_x(new_scroll_x);
        }
    }

    /// 处理双击事件
    fn handle_double_clicked(&mut self, pos: iced_core::Point) {
        if self.is_inside_canvas(pos) {
            if let Some((index, _)) = self.hit_test_note(pos) {
                self.delete_note_by_index(index);
            }
        }
    }

    /// 处理删除键按下事件
    fn handle_delete_pressed(&mut self) {
        if let Some((index, _)) = self.hover_state {
            self.delete_note_by_index(index);
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

    /// 删除指定索引的音符
    ///
    /// # Arguments
    /// * `index` - 音符在 notes 列表中的索引
    pub fn delete_note_by_index(&mut self, index: usize) {
        if index < self.notes.len() {
            let note = self.notes.remove(index);
            tracing::debug!(
                "Editor: deleted note at index {} (tick={}, key={})",
                index,
                note.tick,
                note.key
            );

            // 更新当前音轨的存储
            if !self.notes.is_empty() {
                self.track_notes
                    .insert(self.current_track, self.notes.clone());
            } else {
                // 如果音符列表为空，从 track_notes 中移除该音轨
                self.track_notes.remove(&self.current_track);
            }

            // 清除悬停状态（如果被删除的音符正好是悬停的）
            if let Some((hover_index, _)) = self.hover_state {
                if hover_index == index {
                    self.hover_state = None;
                } else if hover_index > index {
                    // 如果被删除的音符在悬停音符之前，调整索引
                    self.hover_state = Some((hover_index - 1, self.hover_state.unwrap().1));
                }
            }

            // 清除网格缓存以强制重绘
            self.grid_cache.clear();
        }
    }

    /// 删除鼠标位置下的音符（如果存在）
    ///
    /// # Arguments
    /// * `pos` - 鼠标位置
    /// # Returns
    /// 是否删除了音符
    pub fn delete_note_at(&mut self, pos: Point) -> bool {
        if let Some((index, _)) = self.hit_test_note(pos) {
            self.delete_note_by_index(index);
            true
        } else {
            false
        }
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
    pub fn get_note_instances(
        &self,
        theme: &crate::Theme,
        _sidebar_width: f32,
    ) -> Vec<NoteInstance> {
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
                EditState::Dragging { note_index, .. }
                | EditState::ResizingStart { note_index, .. }
                | EditState::ResizingEnd { note_index, .. }
                    if note_index == i =>
                {
                    active_color
                }
                EditState::Idle if self.hover_state.is_some_and(|(idx, _)| idx == i) => hover_color,
                _ => default_color,
            };

            let mut instance = note.to_instance(&self.state, color);
            // 转换为窗口坐标：加上 Canvas 偏移
            instance.position[0] += self.canvas_offset.x;
            instance.position[1] += self.canvas_offset.y;
            instances.push(instance);
        }

        // 渲染正在绘制的音符
        if let EditState::Drawing {
            start_tick,
            key,
            current_tick,
        } = self.edit_state
        {
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
                let local_pos =
                    Point::new(pos.x - self.canvas_offset.x, pos.y - self.canvas_offset.y);
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
        let actions = std::mem::take(&mut self.pending_audio_actions);
        if !actions.is_empty() {
            tracing::debug!("Editor: 取出了 {} 个音频动作", actions.len());
        }
        actions
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
        // 计算实际可滚动的最大范围：总宽度 - 视口宽度
        let total_width = self.state.total_ticks as f32 * self.state.zoom_x;
        let viewport_width = (self.canvas_size.x - self.state.keyboard_width).max(0.0);
        let effective_max_scroll = (total_width - viewport_width).max(0.0);
        self.state.scroll_x = scroll_x.max(0.0).min(effective_max_scroll);
        self.grid_cache.clear();
    }

    pub fn set_scroll_y(&mut self, scroll_y: f32) {
        // 计算实际可滚动的最大范围：总高度 - 视口高度
        let total_height = self.state.visible_key_count as f32 * self.state.zoom_y;
        let viewport_height = self.canvas_size.y.max(0.0);
        let effective_max_scroll = (total_height - viewport_height).max(0.0);
        self.state.scroll_y = scroll_y.max(0.0).min(effective_max_scroll);
        self.grid_cache.clear();
    }

    pub fn set_zoom_x(&mut self, zoom_x: f32, fixed_ratio: f32) {
        let old_zoom_x = self.state.zoom_x;
        self.state.zoom_x = zoom_x.clamp(0.001, 10.0);

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
        self.state.zoom_y = zoom_y.clamp(5.0, 100.0);

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

    // ========== 音轨管理 ==========

    /// 切换到指定音轨（无 MIDI 文件时使用）
    pub fn switch_to_track(&mut self, track_idx: usize) {
        if self.current_track == track_idx {
            return;
        }

        tracing::debug!(
            "Editor: switching from track {} to {}",
            self.current_track,
            track_idx
        );

        // 保存当前音轨的音符
        if !self.notes.is_empty() {
            self.track_notes
                .insert(self.current_track, self.notes.clone());
            tracing::debug!(
                "Editor: saved {} notes for track {}",
                self.notes.len(),
                self.current_track
            );
        }

        // 切换到新音轨
        self.current_track = track_idx;

        // 加载新音轨的音符
        self.notes = self
            .track_notes
            .get(&track_idx)
            .cloned()
            .unwrap_or_default();
        tracing::debug!(
            "Editor: loaded {} notes for track {}",
            self.notes.len(),
            track_idx
        );

        // 清除网格缓存以强制重绘
        self.grid_cache.clear();
    }

    /// 获取当前音轨索引
    pub fn current_track(&self) -> usize {
        self.current_track
    }

    // ========== 洋葱皮功能 ==========

    /// 获取洋葱皮配置的可变引用
    pub fn onion_skin_config_mut(&mut self) -> &mut OnionSkinConfig {
        &mut self.onion_skin_config
    }

    /// 获取洋葱皮配置的引用
    pub fn onion_skin_config(&self) -> &OnionSkinConfig {
        &self.onion_skin_config
    }

    /// 启用洋葱皮
    pub fn enable_onion_skin(&mut self) {
        self.onion_skin_config.enable();
        self.grid_cache.clear();
        tracing::debug!("Editor: onion skin enabled");
    }

    /// 禁用洋葱皮
    pub fn disable_onion_skin(&mut self) {
        self.onion_skin_config.disable();
        self.grid_cache.clear();
        tracing::debug!("Editor: onion skin disabled");
    }

    /// 切换洋葱皮开关
    pub fn toggle_onion_skin(&mut self) {
        self.onion_skin_config.toggle();
        self.grid_cache.clear();
        tracing::debug!(
            "Editor: onion skin toggled, now {}",
            if self.onion_skin_config.is_enabled() {
                "enabled"
            } else {
                "disabled"
            }
        );
    }

    /// 检查洋葱皮是否启用
    pub fn is_onion_skin_enabled(&self) -> bool {
        self.onion_skin_config.is_enabled()
    }

    /// 设置音轨的洋葱皮颜色
    pub fn set_onion_skin_color(&mut self, track_idx: usize, color: iced_core::Color) {
        self.onion_skin_config.set_track_color(track_idx, color);
        self.grid_cache.clear();
    }

    /// 获取音轨的洋葱皮颜色
    pub fn get_onion_skin_color(&self, track_idx: usize) -> iced_core::Color {
        self.onion_skin_config.get_track_color(track_idx)
    }

    /// 设置洋葱皮透明度
    pub fn set_onion_skin_opacity(&mut self, opacity: f32) {
        self.onion_skin_config.set_opacity(opacity);
        self.grid_cache.clear();
    }

    /// 获取洋葱皮透明度
    pub fn onion_skin_opacity(&self) -> f32 {
        self.onion_skin_config.opacity()
    }

    /// 设置是否显示所有音轨的洋葱皮
    pub fn set_onion_skin_show_all(&mut self, show_all: bool) {
        self.onion_skin_config.set_show_all_tracks(show_all);
        self.grid_cache.clear();
    }

    /// 添加可见音轨到洋葱皮
    pub fn add_onion_skin_track(&mut self, track_idx: usize) {
        self.onion_skin_config.add_visible_track(track_idx);
        self.grid_cache.clear();
    }

    /// 从洋葱皮移除音轨
    pub fn remove_onion_skin_track(&mut self, track_idx: usize) {
        self.onion_skin_config.remove_visible_track(track_idx);
        self.grid_cache.clear();
    }

    /// 获取洋葱皮音符实例（用于其他音轨的音符显示）
    ///
    /// # Arguments
    /// * `track_idx` - 音轨索引
    /// * `track_onion_enabled` - 该音轨是否启用了洋葱皮开关
    ///
    /// # Returns
    /// 该音轨的音符实例列表，如果不显示则返回空列表
    pub fn get_onion_skin_instances(
        &self,
        track_idx: usize,
        track_onion_enabled: bool,
    ) -> Vec<NoteInstance> {
        // 检查是否应该显示该音轨的洋葱皮
        if !self
            .onion_skin_config
            .should_show_track(track_idx, track_onion_enabled)
        {
            return Vec::new();
        }

        // 不要显示当前音轨的洋葱皮（当前音轨直接显示）
        if track_idx == self.current_track {
            return Vec::new();
        }

        // 获取该音轨的音符
        let notes = self.track_notes.get(&track_idx);
        if notes.is_none() || notes.unwrap().is_empty() {
            return Vec::new();
        }

        let notes = notes.unwrap();
        let color = self.onion_skin_config.get_track_color(track_idx);
        let mut instances = Vec::with_capacity(notes.len());

        for note in notes {
            let mut instance = note.to_instance(&self.state, color);
            // 转换为窗口坐标
            instance.position[0] += self.canvas_offset.x;
            instance.position[1] += self.canvas_offset.y;
            instances.push(instance);
        }

        instances
    }

    /// 获取所有洋葱皮音符实例（所有其他音轨）
    ///
    /// # Arguments
    /// * `track_onion_states` - 各音轨的洋葱皮开关状态 (track_idx -> is_enabled)
    ///
    /// # Returns
    /// 所有需要显示的洋葱皮音符实例
    ///
    /// # Note
    /// 靠后的音轨（索引大的）会显示在上层（后渲染）
    pub fn get_all_onion_skin_instances(
        &self,
        track_onion_states: &std::collections::HashMap<usize, bool>,
    ) -> Vec<NoteInstance> {
        if !self.is_onion_skin_enabled() {
            return Vec::new();
        }

        let mut all_instances = Vec::new();

        // 收集需要显示的音轨索引并排序（按索引从小到大）
        // 这样索引小的先渲染，索引大的后渲染（显示在上层）
        let mut track_indices: Vec<usize> = track_onion_states
            .iter()
            .filter(|(_, is_enabled)| **is_enabled)
            .map(|(&idx, _)| idx)
            .filter(|&idx| idx != self.current_track) // 排除当前音轨
            .collect();

        // 按索引排序，让靠后的音轨（索引大的）后渲染（上层）
        track_indices.sort();

        for track_idx in track_indices {
            // 音轨开关状态为 true 时才显示
            if let Some(&is_enabled) = track_onion_states.get(&track_idx) {
                let instances = self.get_onion_skin_instances(track_idx, is_enabled);
                all_instances.extend(instances);
            }
        }

        all_instances
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}
