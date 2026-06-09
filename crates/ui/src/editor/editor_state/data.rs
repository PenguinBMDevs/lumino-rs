//! 编辑器数据管理
//!
//! 将 Editor 的数据字段（notes, track_notes, current_track, document, history）
//! 集中管理。

use crate::editor::history;
use crate::editor::note::Note;
use crate::editor::velocity::CcData;
use crate::editor::velocity::widget::TempoPoint;
use lumino_midi_loader::MidiDocument;
use std::collections::HashMap;
use std::sync::Arc;

/// 编辑器数据（音符数据、音轨管理、文档引用、历史记录、CC数据、Tempo数据）
#[derive(Debug)]
pub struct EditorData {
    /// 当前编辑的音符列表
    pub notes: im::Vector<Note>,
    /// 当前编辑的音轨索引
    pub current_track: usize,
    /// 按音轨存储的音符（懒加载缓存，仅保留访问过的音轨）
    pub track_notes: HashMap<usize, im::Vector<Note>>,
    /// MIDI 文档引用（用于懒加载非当前音轨的音符，避免全量预加载导致内存翻倍）
    pub document: Option<Arc<MidiDocument>>,
    /// 历史记录（用于撤销/重做）
    pub history: history::History,
    /// CC 控制器数据
    pub cc_data: CcData,
    /// 从当前 MIDI 文档同步的 Tempo 变化点（用于编辑，初始化为 120BPM 于 tick 0）
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
            history: history::History::new(),
            cc_data: CcData::default(),
            tempo_points: vec![TempoPoint {
                tick: 0.0,
                bpm: 120.0,
            }],
        }
    }
}
