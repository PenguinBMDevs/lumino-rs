pub mod grid;
pub mod history;
pub mod note;
pub mod onion_skin;
pub mod scrollbar_widget;
pub mod spatial_index;
pub mod state;

// 新增子模块
mod clipboard;
mod coords;
mod drag;
mod interaction;
mod note_ops;
mod rendering;
mod scroll;
mod settings;
mod track;

#[cfg(test)]
mod tests;

use crate::{message::AudioAction, toolbar::Tool};
use iced_core::Point;
use iced_widget::canvas;
use lumino_core::storage::config::{AutoScrollConfig, AutoScrollMode};
use lumino_gfx::NoteInstance;
use std::cell::{Cell, RefCell};

use note::Note;
pub use onion_skin::OnionSkinConfig;
pub use state::ViewState;

#[derive(Debug, Clone, Default, PartialEq)]
pub enum EditState {
    #[default]
    Idle,
    /// 框选状态
    Selecting {
        start_pos: Point,
        current_pos: Point,
    },
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
        original_tick: f32,
        original_key: u16,
    },
    ResizingStart {
        note_index: usize,
        original_tick: f32,
        original_length: f32,
    },
    ResizingEnd {
        note_index: usize,
    },
    /// 擦洗状态：在时间轴上拖动来快速定位播放位置
    Scrubbing,
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
    /// 键盘缓存（只随垂直滚动变化）
    pub keyboard_cache: canvas::Cache<crate::Renderer>,
    /// 标尺缓存（只随水平滚动变化）
    pub ruler_cache: canvas::Cache<crate::Renderer>,
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

    /// 当前激活的工具
    current_tool: Tool,
    /// 选中的音符索引集合
    selected_notes: std::collections::HashSet<usize>,

    /// 协作远端用户光标信息（用户ID -> (位置, 颜色, 用户名)）
    pub remote_cursors: std::collections::HashMap<String, (Point, String, String)>,

    /// 历史记录（用于撤销/重做）
    history: history::History,

    /// 演奏指示线位置（以 tick 为单位）
    pub playback_position: f32,

    /// 音符数据是否已变化（需要更新播放管理器）
    notes_changed: bool,

    /// 自动滚动配置
    auto_scroll_config: AutoScrollConfig,

    /// 音符空间索引（惰性更新）
    pub note_index: RefCell<Option<spatial_index::NoteSpatialIndex>>,
    pub note_index_dirty: Cell<bool>,

    /// 其他音轨的音符空间索引（用于洋葱皮等，懒加载）
    pub track_note_indices:
        RefCell<std::collections::HashMap<usize, spatial_index::NoteSpatialIndex>>,
}

impl Editor {
    pub fn new() -> Self {
        let mut editor = Self {
            state: ViewState::default(),
            grid_cache: canvas::Cache::new(),
            keyboard_cache: canvas::Cache::new(),
            ruler_cache: canvas::Cache::new(),
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
            current_tool: Tool::Pointer, // 默认使用框选工具
            selected_notes: std::collections::HashSet::new(),
            remote_cursors: std::collections::HashMap::new(),
            history: history::History::new(),
            playback_position: 0.0,
            notes_changed: false,
            auto_scroll_config: AutoScrollConfig::default(),
            note_index: RefCell::new(None),
            note_index_dirty: Cell::new(true),
            track_note_indices: RefCell::new(std::collections::HashMap::new()),
        };
        editor.max_scroll_x = editor.state.total_ticks as f32 * editor.state.zoom_x;
        editor.max_scroll_y = editor.state.visible_key_count as f32 * editor.state.zoom_y;
        editor
    }

    /// 设置当前工具
    pub fn set_tool(&mut self, tool: Tool) {
        self.current_tool = tool;
        // 切换工具时清除选中状态
        if tool != Tool::Pointer {
            self.selected_notes.clear();
        }
    }

    /// 获取当前工具
    pub fn current_tool(&self) -> Tool {
        self.current_tool
    }

    /// 更新远端鼠标位置
    pub fn update_remote_cursor(
        &mut self,
        user_id: String,
        pos: Point,
        color: String,
        username: String,
    ) {
        self.remote_cursors.insert(user_id, (pos, color, username));
        self.grid_cache.clear();
    }

    /// 移除远端鼠标
    pub fn remove_remote_cursor(&mut self, user_id: &str) {
        self.remote_cursors.remove(user_id);
        self.grid_cache.clear();
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

    /// 检查音符数据是否已变化
    pub fn notes_changed(&self) -> bool {
        self.notes_changed
    }

    /// 清除音符变化标志
    pub fn clear_notes_changed(&mut self) {
        self.notes_changed = false;
    }

    /// 标记音符数据已变化
    pub fn mark_notes_changed(&mut self) {
        self.notes_changed = true;
        self.note_index_dirty.set(true);
    }

    // ===== 自动滚动相关 =====

    /// 设置自动滚动配置
    pub fn set_auto_scroll_config(&mut self, config: AutoScrollConfig) {
        self.auto_scroll_config = config;
    }

    /// 获取自动滚动配置
    pub fn auto_scroll_config(&self) -> &AutoScrollConfig {
        &self.auto_scroll_config
    }

    /// 循环切换自动滚动模式
    pub fn cycle_auto_scroll_mode(&mut self) {
        self.auto_scroll_config.mode = match self.auto_scroll_config.mode {
            AutoScrollMode::FixedIndicatorLeft => AutoScrollMode::ScrollingIndicator,
            AutoScrollMode::ScrollingIndicator => AutoScrollMode::Off,
            AutoScrollMode::Off => AutoScrollMode::FixedIndicatorLeft,
        };
        tracing::debug!(
            "Editor: 自动滚动模式切换为 {:?}",
            self.auto_scroll_config.mode
        );
    }

    /// 获取当前自动滚动模式
    pub fn auto_scroll_mode(&self) -> AutoScrollMode {
        self.auto_scroll_config.mode
    }

    /// 更新自动滚动（在每帧渲染前调用，根据播放位置调整滚动）
    /// 返回是否需要刷新网格缓存
    pub fn update_auto_scroll(&mut self, playback_tick: f32) -> bool {
        if self.auto_scroll_config.mode == AutoScrollMode::Off {
            return false;
        }

        let viewport_width = (self.canvas_size.x - self.state.keyboard_width).max(0.0);
        if viewport_width <= 0.0 {
            return false;
        }

        match self.auto_scroll_config.mode {
            AutoScrollMode::FixedIndicatorLeft => {
                // 模式1：指示线固定在左侧指定位置，卷帘自动左移
                let indicator_pos = self.auto_scroll_config.fixed_indicator_position as f32;
                // 指示线在屏幕上的位置 = keyboard_width + indicator_pos
                // 指示线对应的 tick = scroll_x / zoom_x + (indicator_pos / zoom_x)
                // 所以 scroll_x = playback_tick * zoom_x - indicator_pos
                let target_scroll_x = playback_tick * self.state.zoom_x - indicator_pos;
                self.set_scroll_x(target_scroll_x);
                true
            }
            AutoScrollMode::ScrollingIndicator => {
                // 模式2：指示线跟随播放位置移动，到达右侧触发位置时自动翻页
                let trigger_offset = self.auto_scroll_config.page_trigger_offset as f32;
                let return_pos = self.auto_scroll_config.page_return_position as f32;

                // 计算指示线当前在屏幕上的位置
                let indicator_screen_x = playback_tick * self.state.zoom_x - self.state.scroll_x
                    + self.state.keyboard_width;

                // 计算触发位置（从右边缘算起）
                let trigger_screen_x = viewport_width + self.state.keyboard_width - trigger_offset;

                // 如果指示线超过触发位置，翻页
                if indicator_screen_x >= trigger_screen_x {
                    // 翻页：让指示线回到左侧指定位置
                    let target_scroll_x = playback_tick * self.state.zoom_x - return_pos;
                    self.set_scroll_x(target_scroll_x);
                    return true;
                }

                // 否则指示线正常跟随，不主动滚动（用户可手动滚动）
                false
            }
            AutoScrollMode::Off => false,
        }
    }

    /// 获取演奏指示线在 Canvas 坐标系中的 X 坐标（用于渲染）
    /// 返回 None 表示不需要显示指示线
    pub fn get_playback_indicator_screen_x(&self) -> Option<f32> {
        if self.auto_scroll_config.mode == AutoScrollMode::Off {
            return None;
        }

        match self.auto_scroll_config.mode {
            AutoScrollMode::FixedIndicatorLeft => {
                // 模式1：指示线固定在左侧指定位置（Canvas 坐标系）
                let indicator_pos = self.auto_scroll_config.fixed_indicator_position as f32;
                Some(self.state.keyboard_width + indicator_pos)
            }
            AutoScrollMode::ScrollingIndicator => {
                // 模式2：指示线跟随播放位置（Canvas 坐标系）
                let indicator_x = self.playback_position * self.state.zoom_x - self.state.scroll_x
                    + self.state.keyboard_width;
                Some(indicator_x)
            }
            AutoScrollMode::Off => None,
        }
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

// 重新导出洋葱皮功能
impl Editor {
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
        tracing::debug!("Editor: 洋葱皮已启用");
    }

    /// 禁用洋葱皮
    pub fn disable_onion_skin(&mut self) {
        self.onion_skin_config.disable();
        self.grid_cache.clear();
        tracing::debug!("Editor: 洋葱皮已禁用");
    }

    /// 切换洋葱皮开关
    pub fn toggle_onion_skin(&mut self) {
        self.onion_skin_config.toggle();
        self.grid_cache.clear();
        tracing::info!(
            "Editor: saved {} notes to track {}",
            self.notes.len(),
            self.current_track
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

    /// 将音符列表转换为逻辑实例（GPU 负责坐标变换）
    fn notes_to_instances<'a>(
        &self,
        notes: impl Iterator<Item = &'a Note>,
        color: iced_core::Color,
    ) -> Vec<NoteInstance> {
        let mut instances = Vec::new();
        for note in notes {
            let instance = note.to_instance(color);
            instances.push(instance);
        }
        instances
    }

    /// 获取所有洋葱皮音符原始数据（用于缓存）
    /// 返回 (tick, key, length, color) 元组，不含屏幕坐标
    pub fn get_onion_skin_notes(
        &self,
        track_onion_states: &std::collections::HashMap<usize, bool>,
        visible_tick_start: f32,
        visible_tick_end: f32,
        visible_key_min: u16,
        visible_key_max: u16,
    ) -> Vec<(f32, u16, f32, iced_core::Color)> {
        if !self.is_onion_skin_enabled() {
            return Vec::new();
        }

        let mut track_indices: Vec<usize> = track_onion_states
            .iter()
            .filter(|(_, is_enabled)| **is_enabled)
            .map(|(&idx, _)| idx)
            .filter(|&idx| idx != self.current_track)
            .collect();

        track_indices.sort();

        let mut all_notes = Vec::new();

        for track_idx in track_indices {
            if let Some(&is_enabled) = track_onion_states.get(&track_idx) {
                if !self
                    .onion_skin_config
                    .should_show_track(track_idx, is_enabled)
                {
                    continue;
                }
                if let Some(notes) = self.track_notes.get(&track_idx) {
                    let color = self.onion_skin_config.get_track_color(track_idx);

                    let search_start = (visible_tick_start - 19200.0).max(0.0);
                    let mut indices_map = self.track_note_indices.borrow_mut();
                    let index = indices_map
                        .entry(track_idx)
                        .or_insert_with(|| spatial_index::NoteSpatialIndex::from_notes(notes));

                    // 使用实际的 key 范围进行裁剪，而不是 0..127
                    let candidates = index.query(
                        search_start,
                        visible_tick_end,
                        visible_key_min,
                        visible_key_max,
                    );

                    for &i in &candidates {
                        let note = &notes[i];
                        // 精确的 tick 和 key 裁剪
                        if note.tick + note.length < visible_tick_start
                            || note.tick > visible_tick_end
                        {
                            continue;
                        }
                        if note.key < visible_key_min || note.key > visible_key_max {
                            continue;
                        }
                        all_notes.push((note.tick, note.key, note.length, color));
                    }
                }
            }
        }

        all_notes
    }

    /// 获取洋葱皮音符实例（用于其他音轨的音符显示）
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
        let Some(notes) = self.track_notes.get(&track_idx) else {
            return Vec::new();
        };
        if notes.is_empty() {
            return Vec::new();
        }

        let color = self.onion_skin_config.get_track_color(track_idx);
        self.notes_to_instances(notes.iter(), color)
    }

    /// 获取所有洋葱皮音符实例（所有其他音轨）
    pub fn get_all_onion_skin_instances(
        &self,
        track_onion_states: &std::collections::HashMap<usize, bool>,
    ) -> Vec<NoteInstance> {
        if !self.is_onion_skin_enabled() {
            return Vec::new();
        }

        // 收集需要显示的音轨索引并按从小到大排序
        // 索引小的先渲染，索引大的后渲染（显示在上层）
        let mut track_indices: Vec<usize> = track_onion_states
            .iter()
            .filter(|(_, is_enabled)| **is_enabled)
            .map(|(&idx, _)| idx)
            .filter(|&idx| idx != self.current_track)
            .collect();

        track_indices.sort();

        let mut all_instances = Vec::new();
        for track_idx in track_indices {
            if let Some(&is_enabled) = track_onion_states.get(&track_idx) {
                all_instances.extend(self.get_onion_skin_instances(track_idx, is_enabled));
            }
        }

        all_instances
    }

    /// Push current state to history
    pub fn push_history(&mut self) {
        let snapshot = history::EditorSnapshot::new(self.notes.clone(), self.current_track);
        tracing::debug!(
            "推送历史记录: {} 个音符，音轨 {}",
            snapshot.notes.len(),
            snapshot.current_track
        );
        self.history.push(snapshot);
    }

    /// Undo the last action
    pub fn undo(&mut self) -> bool {
        let current_state = history::EditorSnapshot::new(self.notes.clone(), self.current_track);
        tracing::info!(
            "尝试撤销: 当前音符数 = {}, 可撤销 = {}",
            self.notes.len(),
            self.can_undo()
        );

        if let Some(snapshot) = self.history.undo(current_state) {
            self.notes = snapshot.notes;
            self.current_track = snapshot.current_track;
            self.grid_cache.clear();
            self.note_index_dirty.set(true);
            self.track_note_indices.borrow_mut().clear();
            tracing::info!("撤销操作成功: {} 个音符", self.notes.len());
            true
        } else {
            tracing::info!("没有可撤销的操作");
            false
        }
    }

    /// Redo the last undone action
    pub fn redo(&mut self) -> bool {
        let current_state = history::EditorSnapshot::new(self.notes.clone(), self.current_track);

        if let Some(snapshot) = self.history.redo(current_state) {
            self.notes = snapshot.notes;
            self.current_track = snapshot.current_track;
            self.grid_cache.clear();
            self.note_index_dirty.set(true);
            self.track_note_indices.borrow_mut().clear();
            tracing::info!("重做操作成功");
            true
        } else {
            tracing::info!("没有可重做的操作");
            false
        }
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// 生成网格线实例（用于 wgpu 渲染）
    pub fn get_grid_line_instances(
        &self,
        bar_color: iced_core::Color,
        beat_color: iced_core::Color,
        half_beat_color: iced_core::Color,
        grid_color: iced_core::Color,
        key_line_color: iced_core::Color,
    ) -> Vec<lumino_gfx::GridLineInstance> {
        use lumino_gfx::GridLineInstance;

        let mut instances = Vec::new();
        let view = &self.state;
        let ppq = view.ppq as f32;
        let keyboard_width = view.keyboard_width;
        let ruler_height = view.ruler_height;

        // 计算可见范围
        let canvas_width = self.canvas_size.x;
        let canvas_height = self.canvas_size.y;

        // ===== 纵向网格线（小节线、拍线） =====
        let measure_ticks = ppq * 4.0;
        let start_tick = view.scroll_x / view.zoom_x;
        let end_tick = (view.scroll_x + canvas_width - keyboard_width) / view.zoom_x;
        let grid_gap = ppq / 4.0; // 十六分音符精度

        let mut current_tick = (start_tick / grid_gap).ceil() * grid_gap;

        while current_tick < end_tick {
            let screen_x = (current_tick * view.zoom_x) - view.scroll_x
                + keyboard_width
                + self.canvas_offset.x;

            // 只生成在 Canvas 区域内的线条
            if screen_x >= self.canvas_offset.x + keyboard_width
                && screen_x <= self.canvas_offset.x + canvas_width
            {
                let is_measure = (current_tick % measure_ticks).abs() < 0.1;
                let is_beat = (current_tick % ppq).abs() < 0.1;
                let is_half_beat = (current_tick % (ppq / 2.0)).abs() < 0.1;

                let (color, width) = if is_measure {
                    ([bar_color.r, bar_color.g, bar_color.b, bar_color.a], 4.0)
                } else if is_beat {
                    (
                        [beat_color.r, beat_color.g, beat_color.b, beat_color.a],
                        1.0,
                    )
                } else if is_half_beat {
                    (
                        [
                            half_beat_color.r,
                            half_beat_color.g,
                            half_beat_color.b,
                            half_beat_color.a,
                        ],
                        0.5,
                    )
                } else {
                    (
                        [grid_color.r, grid_color.g, grid_color.b, grid_color.a],
                        0.5,
                    )
                };

                instances.push(GridLineInstance::new(
                    [screen_x, self.canvas_offset.y + ruler_height],
                    [screen_x, self.canvas_offset.y + canvas_height],
                    color,
                    width,
                ));
            }
            current_tick += grid_gap;
        }

        // ===== 横向网格线（琴键分隔线） =====
        let max_key_index = (view.visible_key_count.saturating_sub(1)) as f32;

        for i in 0..view.visible_key_count {
            let keynum = i as isize;
            let world_y = (max_key_index - keynum as f32) * view.zoom_y;
            let screen_y = world_y - view.scroll_y + ruler_height + self.canvas_offset.y;

            // 只生成在 Canvas 区域内的线条
            if screen_y >= self.canvas_offset.y + ruler_height
                && screen_y <= self.canvas_offset.y + canvas_height
            {
                instances.push(GridLineInstance::new(
                    [self.canvas_offset.x + keyboard_width, screen_y],
                    [self.canvas_offset.x + canvas_width, screen_y],
                    [
                        key_line_color.r,
                        key_line_color.g,
                        key_line_color.b,
                        key_line_color.a,
                    ],
                    1.0,
                ));
            }
        }

        instances
    }
}
