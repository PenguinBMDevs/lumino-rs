//! 编辑器状态与业务逻辑
//!
//! 包含 EditorData、InteractionState、CanvasState 以及所有音符操作业务逻辑。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::history::{EditorSnapshot, History};
use crate::midi_types::{CcData, TempoPoint};
use crate::note::Note;
use crate::storage::config::{AutoScrollConfig, EraserBehavior, SelectionBoxMode};
use crate::view_state::ViewState;
use lumino_message::{AudioAction, Tool};

// ─── CanvasState ───

/// Canvas 状态（尺寸和偏移）
#[derive(Debug, Clone, Copy, Default)]
pub struct CanvasState {
    /// Canvas 在窗口中的偏移量（用于坐标转换）
    pub offset_x: f32,
    pub offset_y: f32,
    /// Canvas 尺寸（宽, 高）
    pub size_x: f32,
    pub size_y: f32,
    /// 当前鼠标在窗口中的位置
    pub cursor_position: Option<(f32, f32)>,
}

impl CanvasState {
    pub fn new() -> Self {
        Self::default()
    }
}

// ─── 交互状态 ───

/// 编辑状态
#[derive(Debug, Clone, Default, PartialEq)]
pub enum EditState {
    #[default]
    Idle,
    Selecting {
        start_tick: f32,
        start_key: u16,
        current_tick: f32,
        current_key: u16,
    },
    Drawing {
        start_tick: f32,
        key: u16,
        current_tick: f32,
    },
    PendingDrag {
        note_index: usize,
        start_pos: (f32, f32),
        original_tick: f32,
        original_key: u16,
    },
    Dragging {
        note_index: usize,
        offset_tick: f32,
        offset_key: i32,
        last_played_key: u16,
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
    DraggingSelection {
        last_tick: f32,
        last_key: u16,
    },
    ResizingSelectionStart {
        last_tick: f32,
    },
    ResizingSelectionEnd {
        last_tick: f32,
    },
    Scrubbing,
}

/// 点击命中类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HitType {
    Start,
    Middle,
    End,
}

/// 选择框命中类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SelectionHitType {
    Inside,
    LeftEdge,
    RightEdge,
}

/// 交互状态
#[derive(Debug, Default)]
pub struct InteractionState {
    pub edit_state: EditState,
    pub hover_state: Option<(usize, HitType)>,
    pub selected_notes: HashSet<usize>,
    /// 待处理的音频动作
    pub pending_audio_actions: Vec<AudioAction>,
}

impl InteractionState {
    /// 获取并清空待处理的音频动作
    pub fn take_audio_actions(&mut self) -> Vec<AudioAction> {
        std::mem::take(&mut self.pending_audio_actions)
    }

    /// 添加音频动作
    pub fn push_audio_action(&mut self, action: AudioAction) {
        self.pending_audio_actions.push(action);
    }

    /// 添加播放音符的音频动作
    pub fn play_note_audio(&mut self, key: u16, velocity: u8) {
        self.pending_audio_actions.push(AudioAction::PlayNote {
            key: key as u8,
            velocity,
        });
    }
}

// ─── EditorData ───

/// 编辑器数据
#[derive(Debug)]
pub struct EditorData {
    pub notes: im::Vector<Note>,
    pub current_track: usize,
    pub track_notes: HashMap<usize, im::Vector<Note>>,
    pub document: Option<Arc<lumino_midi_loader::MidiDocument>>,
    pub history: History,
    pub cc_data: CcData,
    pub tempo_points: Vec<TempoPoint>,
}

impl Default for EditorData {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorData {
    pub fn new() -> Self {
        Self {
            notes: im::Vector::new(),
            current_track: 0,
            track_notes: HashMap::new(),
            document: None,
            history: History::new(),
            cc_data: CcData::default(),
            tempo_points: vec![TempoPoint {
                tick: 0.0,
                bpm: 120.0,
            }],
        }
    }

    /// 重置编辑器数据到初始状态（释放所有内存）
    pub fn reset(&mut self) {
        self.notes.clear();
        self.track_notes.clear();
        self.current_track = 0;
        self.history.clear();
        self.document = None;
        self.cc_data = CcData::default();
        self.tempo_points = vec![TempoPoint {
            tick: 0.0,
            bpm: 120.0,
        }];
    }

    // ── 历史记录 ──

    pub fn push_history(&mut self) {
        self.history
            .push(EditorSnapshot::new(self.notes.clone(), self.current_track));
    }

    pub fn undo(&mut self) -> bool {
        let current = EditorSnapshot::new(self.notes.clone(), self.current_track);
        if let Some(snapshot) = self.history.undo(current) {
            self.notes = snapshot.notes;
            self.current_track = snapshot.current_track;
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        let current = EditorSnapshot::new(self.notes.clone(), self.current_track);
        if let Some(snapshot) = self.history.redo(current) {
            self.notes = snapshot.notes;
            self.current_track = snapshot.current_track;
            true
        } else {
            false
        }
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// 同步 notes 到 track_notes 缓存
    pub fn sync_track_notes(&mut self) {
        if self.notes.is_empty() {
            self.track_notes.remove(&self.current_track);
        } else {
            self.track_notes
                .insert(self.current_track, self.notes.clone());
        }
    }

    // ── 音符操作 ──

    pub fn delete_note_by_index(&mut self, index: usize) {
        if index < self.notes.len() {
            self.push_history();
            self.notes.remove(index);
            self.sync_track_notes();
        }
    }

    pub fn delete_selected_notes(&mut self, selected: &HashSet<usize>) {
        if selected.is_empty() {
            return;
        }
        self.push_history();
        let mut indices: Vec<usize> = selected.iter().copied().collect();
        indices.sort_by(|a, b| b.cmp(a));
        for &i in &indices {
            if i < self.notes.len() {
                self.notes.remove(i);
            }
        }
        self.sync_track_notes();
    }

    pub fn select_all_notes(&self) -> HashSet<usize> {
        (0..self.notes.len()).collect()
    }

    /// 分割音符
    pub fn split_note(&mut self, index: usize, split_tick: f32) -> bool {
        if index >= self.notes.len() {
            return false;
        }
        let (note_tick, note_length, key, velocity, channel) = {
            let n = &self.notes[index];
            if split_tick <= n.tick || split_tick >= n.tick + n.length {
                return false;
            }
            (n.tick, n.length, n.key, n.velocity, n.channel)
        };
        self.push_history();
        self.notes.remove(index);
        let right = Note::from_raw(
            split_tick,
            key,
            note_tick + note_length - split_tick,
            velocity,
            channel,
        );
        self.notes.insert(index, right);
        let left = Note::from_raw(note_tick, key, split_tick - note_tick, velocity, channel);
        self.notes.insert(index, left);
        self.sync_track_notes();
        true
    }

    /// 合并选中音符
    pub fn glue_selected_notes(&mut self, selected: &HashSet<usize>) -> usize {
        let sel: Vec<usize> = selected.iter().copied().collect();
        if sel.is_empty() {
            return 0;
        }
        type NT = (usize, f32, u16, f32, u8, u8);
        let mut sn: Vec<NT> = sel
            .iter()
            .filter_map(|&i| {
                self.notes
                    .get(i)
                    .map(|n| (i, n.tick, n.key, n.length, n.velocity, n.channel))
            })
            .collect();
        if sn.is_empty() {
            return 0;
        }
        sn.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        let mut groups: Vec<Vec<NT>> = Vec::new();
        for note in &sn {
            let added = match groups.last_mut() {
                Some(g) => match g.last() {
                    Some(last) if last.2 == note.2 && note.1 <= last.1 + last.3 + 1.0 => {
                        g.push(*note);
                        true
                    }
                    _ => false,
                },
                None => false,
            };
            if !added {
                groups.push(vec![*note]);
            }
        }
        let groups: Vec<Vec<NT>> = groups.into_iter().filter(|g| g.len() >= 2).collect();
        if groups.is_empty() {
            return 0;
        }
        self.push_history();
        let mut merged = 0usize;
        for group in &groups {
            let first = &group[0];
            let last = &group[group.len() - 1];
            let merged_tick = first.1;
            let merged_length = (last.1 + last.3) - merged_tick;
            let rm: Vec<usize> = group.iter().map(|n| n.0).collect();
            let mut rm_sorted = rm.clone();
            rm_sorted.sort_by(|a, b| b.cmp(a));
            for &idx in &rm_sorted {
                self.notes.remove(idx);
            }
            let adj = rm[0].min(self.notes.len());
            self.notes.insert(
                adj,
                Note::from_raw(merged_tick, first.2, merged_length, first.4, first.5),
            );
            merged += 1;
        }
        self.sync_track_notes();
        merged
    }

    /// 完成绘制新音符（纯业务逻辑），返回创建的 Note
    pub fn finish_drawing(
        &mut self,
        start_tick: f32,
        key: u16,
        current_tick: f32,
        snap_precision: f32,
        default_note_length: f32,
    ) -> Option<Note> {
        if self.current_track == 0 {
            tracing::debug!("编辑器: Conductor 轨道禁止放置音符");
            return None;
        }
        let (tick, length) = if current_tick > start_tick {
            (start_tick, current_tick - start_tick)
        } else if current_tick < start_tick {
            (current_tick, start_tick - current_tick)
        } else {
            (start_tick, default_note_length)
        };
        let length = length.max(snap_precision);
        self.push_history();
        let note = Note::new(tick, key, length);
        self.notes.push_back(note.clone());
        self.track_notes
            .insert(self.current_track, self.notes.clone());
        tracing::debug!(
            "编辑器: 已保存 {} 个音符到音轨 {}",
            self.notes.len(),
            self.current_track
        );
        Some(note)
    }

    /// 计算选择框内的音符索引
    pub fn compute_selection(
        &self,
        start_tick: f32,
        start_key: u16,
        current_tick: f32,
        current_key: u16,
    ) -> HashSet<usize> {
        let min_tick = start_tick.min(current_tick);
        let max_tick = start_tick.max(current_tick);
        let min_key = start_key.min(current_key);
        let max_key = start_key.max(current_key);
        let mut selected = HashSet::new();
        for (i, note) in self.notes.iter().enumerate() {
            let note_end = note.tick + note.length;
            if note.key >= min_key
                && note.key <= max_key
                && note.tick < max_tick
                && note_end > min_tick
            {
                selected.insert(i);
            }
        }
        selected
    }

    /// 获取选择框内的音符索引列表
    pub fn get_notes_in_selection_box(
        &self,
        start_tick: f32,
        start_key: u16,
        current_tick: f32,
        current_key: u16,
    ) -> Vec<usize> {
        let ts = start_tick.min(current_tick);
        let te = start_tick.max(current_tick);
        let km = start_key.min(current_key);
        let kx = start_key.max(current_key);
        let mut r = Vec::new();
        for (i, n) in self.notes.iter().enumerate() {
            let ne = n.tick + n.length;
            if n.key >= km && n.key <= kx && n.tick < te && ne > ts {
                r.push(i);
            }
        }
        r
    }
}

// ─── EditorState（完整编辑器状态） ───

/// 编辑器完整状态（包含所有业务逻辑）
#[derive(Debug)]
pub struct EditorState {
    pub view: ViewState,
    pub canvas: CanvasState,
    pub interaction: InteractionState,
    pub tool: Tool,
    pub auto_scroll: AutoScrollConfig,
    pub max_scroll: (f32, f32),
    pub data: EditorData,
}

impl Default for EditorState {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorState {
    pub fn new() -> Self {
        let view = ViewState::default();
        Self {
            max_scroll: (
                view.total_ticks as f32 * view.zoom_x,
                view.visible_key_count as f32 * view.zoom_y,
            ),
            view,
            canvas: CanvasState::default(),
            interaction: InteractionState::default(),
            data: EditorData::new(),
            tool: Tool::Pointer,
            auto_scroll: AutoScrollConfig::default(),
        }
    }

    /// 重置编辑器状态到初始值（释放所有内存）
    pub fn reset(&mut self) {
        self.data.reset();
        self.interaction = InteractionState::default();
        self.view = ViewState::default();
        self.tool = Tool::Pointer;
        self.auto_scroll = AutoScrollConfig::default();
        self.max_scroll = (
            self.view.total_ticks as f32 * self.view.zoom_x,
            self.view.visible_key_count as f32 * self.view.zoom_y,
        );
    }

    pub fn update_max_scroll(&mut self, total_ticks: u32) {
        self.max_scroll = (
            total_ticks as f32 * self.view.zoom_x,
            self.view.visible_key_count as f32 * self.view.zoom_y,
        );
    }

    pub fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
        if tool != Tool::Pointer {
            self.interaction.selected_notes.clear();
        }
    }

    pub fn current_tool(&self) -> Tool {
        self.tool
    }

    pub fn set_scroll_x(&mut self, scroll_x: f32, keyboard_width: f32, canvas_width: f32) {
        let tw = self.view.total_ticks as f32 * self.view.zoom_x;
        let vw = (canvas_width - keyboard_width).max(0.0);
        let ms = (tw - vw).max(0.0);
        self.view.scroll_x = scroll_x.max(0.0).min(ms);
        self.view.smooth_scroll.target_x = self.view.scroll_x;
        self.view.smooth_scroll.active = false;
    }

    pub fn set_scroll_y(&mut self, scroll_y: f32, canvas_height: f32) {
        let th = self.view.visible_key_count as f32 * self.view.zoom_y;
        let vh = (canvas_height - self.view.ruler_height).max(0.0);
        let ms = (th - vh).max(0.0);
        self.view.scroll_y = scroll_y.max(0.0).min(ms);
        self.view.smooth_scroll.target_y = self.view.scroll_y;
        self.view.smooth_scroll.active = false;
    }

    pub fn set_zoom_x(
        &mut self,
        zoom_x: f32,
        fixed_ratio: f32,
        keyboard_width: f32,
        canvas_width: f32,
        min_zoom: f32,
        max_zoom: f32,
    ) {
        let old = self.view.zoom_x;
        self.view.zoom_x = zoom_x.clamp(min_zoom, max_zoom);
        let ratio = self.view.zoom_x / old;
        let vw = (canvas_width - keyboard_width).max(0.0);
        let fp = self.view.scroll_x + vw * fixed_ratio;
        self.view.scroll_x = fp * ratio - vw * fixed_ratio;
        self.update_max_scroll(self.view.total_ticks);
        let ms = (self.max_scroll.0 - vw).max(0.0);
        self.view.scroll_x = self.view.scroll_x.max(0.0).min(ms);
    }

    pub fn set_zoom_y(
        &mut self,
        zoom_y: f32,
        fixed_ratio: f32,
        canvas_height: f32,
        min_zoom: f32,
        max_zoom: f32,
    ) {
        let old = self.view.zoom_y;
        self.view.zoom_y = zoom_y.clamp(min_zoom, max_zoom);
        let ratio = self.view.zoom_y / old;
        let vh = canvas_height.max(0.0);
        let fp = self.view.scroll_y + vh * fixed_ratio;
        self.view.scroll_y = fp * ratio - vh * fixed_ratio;
        self.update_max_scroll(self.view.total_ticks);
        let vh2 = (canvas_height - self.view.ruler_height).max(0.0);
        let ms = (self.max_scroll.1 - vh2).max(0.0);
        self.view.scroll_y = self.view.scroll_y.max(0.0).min(ms);
    }

    pub fn set_visible_key_count(
        &mut self,
        count: u16,
        min_count: u16,
        max_count: u16,
        canvas_height: f32,
    ) {
        self.view.visible_key_count = count.clamp(min_count, max_count);
        self.update_max_scroll(self.view.total_ticks);
        let vh = (canvas_height - self.view.ruler_height).max(0.0);
        let ms = (self.max_scroll.1 - vh).max(0.0);
        if self.view.scroll_y > ms {
            self.view.scroll_y = ms;
        }
    }

    pub fn set_keyboard_width(&mut self, width: f32) {
        self.view.keyboard_width = width.max(0.0);
    }
    pub fn set_snap_precision(&mut self, precision: f32) {
        self.view.snap_precision = precision.max(1.0);
    }
    pub fn set_default_note_length(&mut self, length: f32) {
        self.view.default_note_length = length.max(1.0);
    }
    pub fn set_eraser_behavior(&mut self, behavior: EraserBehavior) {
        self.view.eraser_behavior = behavior;
    }
    pub fn set_selection_box_mode(&mut self, mode: SelectionBoxMode) {
        self.view.selection_box_mode = mode;
    }

    // ── 碰撞检测 ──

    pub fn hit_test_note(
        &self,
        pos: (f32, f32),
        edge_threshold_px: f32,
    ) -> Option<(usize, HitType)> {
        let tick = self.view.x_to_tick(pos.0);
        let key = self.view.y_to_key(pos.1);
        for (i, note) in self.data.notes.iter().enumerate().rev() {
            if note.key == key && tick >= note.tick && tick <= note.tick + note.length {
                let sd = (tick - note.tick).abs();
                let ed = (tick - (note.tick + note.length)).abs();
                let et = edge_threshold_px / self.view.zoom_x;
                if ed < et {
                    return Some((i, HitType::End));
                }
                if sd < et {
                    return Some((i, HitType::Start));
                }
                return Some((i, HitType::Middle));
            }
        }
        None
    }

    pub fn get_selection_box_bounds(&self) -> Option<(f32, f32, f32, f32)> {
        let sel = &self.interaction.selected_notes;
        if sel.is_empty() {
            return None;
        }
        let mut min_t = f32::INFINITY;
        let mut max_te = f32::NEG_INFINITY;
        let mut max_k = u16::MIN;
        let mut min_k = u16::MAX;
        for &i in sel.iter() {
            if let Some(n) = self.data.notes.get(i) {
                min_t = min_t.min(n.tick);
                max_te = max_te.max(n.tick + n.length);
                max_k = max_k.max(n.key);
                min_k = min_k.min(n.key);
            }
        }
        if min_t.is_infinite() {
            return None;
        }
        Some((
            self.view.tick_to_x(min_t),
            self.view.tick_to_x(max_te),
            self.view.key_to_y(max_k),
            self.view.key_to_y(min_k) + self.view.zoom_y,
        ))
    }

    pub fn hit_test_selection_box(&self, pos: (f32, f32)) -> Option<SelectionHitType> {
        let (min_x, max_x, min_y, max_y) = self.get_selection_box_bounds()?;
        if pos.0 < min_x || pos.0 > max_x || pos.1 < min_y || pos.1 > max_y {
            return None;
        }
        let et = 4.0f32;
        let ol = (pos.0 - min_x).abs() < et;
        let orr = (pos.0 - max_x).abs() < et;
        if ol && !orr {
            return Some(SelectionHitType::LeftEdge);
        }
        if orr && !ol {
            return Some(SelectionHitType::RightEdge);
        }
        Some(SelectionHitType::Inside)
    }
}

impl EditorState {
    /// 获取选择框内的音符索引列表（委托到 EditorData）
    pub fn get_notes_in_selection_box(
        &self,
        start_tick: f32,
        start_key: u16,
        current_tick: f32,
        current_key: u16,
    ) -> Vec<usize> {
        self.data
            .get_notes_in_selection_box(start_tick, start_key, current_tick, current_key)
    }

    /// 计算选择框内的音符索引（委托到 EditorData）
    pub fn compute_selection(
        &self,
        start_tick: f32,
        start_key: u16,
        current_tick: f32,
        current_key: u16,
    ) -> HashSet<usize> {
        self.data
            .compute_selection(start_tick, start_key, current_tick, current_key)
    }

    // ── 交互业务逻辑 ──

    /// 开始编辑现有音符
    pub fn start_note_edit(&mut self, index: usize, hit_type: HitType, pos: (f32, f32)) {
        match hit_type {
            HitType::Start => {
                self.data.push_history();
                let note = &self.data.notes[index];
                self.interaction.edit_state = EditState::ResizingStart {
                    note_index: index,
                    original_tick: note.tick,
                    original_length: note.length,
                };
            }
            HitType::End => {
                self.data.push_history();
                self.interaction.edit_state = EditState::ResizingEnd { note_index: index };
            }
            HitType::Middle => {
                let note = &self.data.notes[index];
                self.interaction.edit_state = EditState::PendingDrag {
                    note_index: index,
                    start_pos: pos,
                    original_tick: note.tick,
                    original_key: note.key,
                };
                self.interaction.play_note_audio(note.key, 100);
            }
        }
    }

    /// 开始绘制新音符
    pub fn start_drawing(&mut self, snapped_tick: f32, key: u16) {
        self.interaction.edit_state = EditState::Drawing {
            start_tick: snapped_tick,
            key,
            current_tick: snapped_tick,
        };
        self.interaction.play_note_audio(key, 100);
    }

    /// 应用音符变化（单音符编辑），返回是否发生了变更
    pub fn apply_note_changes(
        &mut self,
        new_tick: Option<f32>,
        new_key: Option<u16>,
        new_length: Option<f32>,
    ) -> bool {
        let note_index = match self.interaction.edit_state {
            EditState::Dragging { note_index, .. }
            | EditState::ResizingStart { note_index, .. }
            | EditState::ResizingEnd { note_index, .. } => note_index,
            EditState::DraggingSelection { .. }
            | EditState::ResizingSelectionStart { .. }
            | EditState::ResizingSelectionEnd { .. } => return false,
            _ => return false,
        };

        if let Some(note) = self.data.notes.get_mut(note_index) {
            let mut changed = false;
            if let Some(t) = new_tick {
                note.tick = t;
                changed = true;
            }
            if let Some(k) = new_key {
                note.key = k;
                changed = true;
            }
            if let Some(l) = new_length {
                note.length = l;
                changed = true;
            }
            return changed;
        }
        false
    }

    /// 处理删除键按下事件，返回被删除音符的索引
    pub fn handle_delete_pressed(&mut self) -> Option<usize> {
        if let Some((index, _)) = self.interaction.hover_state {
            self.data.delete_note_by_index(index);
            Some(index)
        } else {
            None
        }
    }
}
