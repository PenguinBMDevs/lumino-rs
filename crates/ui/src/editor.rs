pub mod grid;
pub mod history;
pub mod note;
pub mod onion_skin;
pub mod scrollbar_widget;
pub mod state;

// 新增子模块
mod coords;
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
use lumino_gfx::NoteInstance;

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
            current_tool: Tool::Pointer, // 默认使用框选工具
            selected_notes: std::collections::HashSet::new(),
            remote_cursors: std::collections::HashMap::new(),
            history: history::History::new(),
            playback_position: 0.0,
            notes_changed: false,
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
        let notes = self.track_notes.get(&track_idx);
        let notes = match notes {
            Some(notes) if !notes.is_empty() => notes,
            _ => return Vec::new(),
        };
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
}
