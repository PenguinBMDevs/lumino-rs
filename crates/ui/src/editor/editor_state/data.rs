//! 编辑器数据管理
//!
//! 将 Editor 的数据字段（notes, track_notes, current_track, document, history）
//! 集中管理。

use crate::editor::history;
use crate::editor::note::Note;
use lumino_core::midi::MidiDocument;
use lumino_gfx::NoteInstance;
use std::collections::HashMap;
use std::sync::Arc;

/// 编辑器数据（音符数据、音轨管理、文档引用、历史记录）
#[derive(Debug)]
pub struct EditorData {
    /// 当前编辑的音符列表
    pub notes: im::Vector<Note>,
    /// 当前编辑的音轨索引
    pub current_track: usize,
    /// 按音轨存储的音符（懒加载缓存，仅保留访问过的音轨）
    pub track_notes: HashMap<usize, im::Vector<Note>>,
    /// 按音轨缓存的洋葱皮 NoteInstance 全量数据（视口无关，跨帧复用）
    ///
    /// 与 cached_all_main_note_instances 同理：
    /// - 首次查询时从 document 构建全量
    /// - 视口变化时从缓存二分查找 + 过滤，避免 document 重新查询+转换
    /// - 在 mark_notes_changed / note_ops 中与 track_notes 同步清除
    pub cached_onion_track_instances: HashMap<usize, Vec<NoteInstance>>,
    /// MIDI 文档引用（用于懒加载非当前音轨的音符，避免全量预加载导致内存翻倍）
    pub document: Option<Arc<MidiDocument>>,
    /// 历史记录（用于撤销/重做）
    pub history: history::History,
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
            cached_onion_track_instances: HashMap::new(),
            document: None,
            history: history::History::new(),
        }
    }
}
