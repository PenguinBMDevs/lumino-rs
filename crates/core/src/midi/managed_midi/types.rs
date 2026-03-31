//! 内存管理 MIDI 加载器类型定义

use serde::{Deserialize, Serialize};

/// 音轨在内存中的存储状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackLocation {
    /// 全部在内存中（含力度>1的音符的音轨）
    InMemory,
    /// 全部在磁盘上
    OnDisk,
    /// 按需加载后暂留内存
    LoadedFromDisk,
}

/// 单个音轨的摘要信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackSummary {
    pub track_index: usize,
    pub event_count: u64,
    pub note_count: u64,
    /// 力度 > 1 的音符数
    pub high_vel_note_count: u64,
    pub max_tick: u32,
    pub memory_bytes: usize,
    pub location: TrackLocationSerde,
}

/// 可序列化的音轨位置
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackLocationSerde {
    InMemory,
    OnDisk,
}

/// 管理器统计信息
#[derive(Debug, Clone)]
pub struct ManagerStats {
    pub track_count: usize,
    pub in_memory_track_count: usize,
    pub on_disk_track_count: usize,
    pub loaded_track_count: usize,
    pub base_memory_bytes: usize,
    pub loaded_memory_bytes: usize,
    pub total_memory_bytes: usize,
    pub memory_limit_bytes: usize,
    pub total_notes: u64,
    pub high_velocity_notes: u64,
}

impl std::fmt::Display for ManagerStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "MIDI 内存管理器统计:")?;
        writeln!(f, "  总音轨: {}", self.track_count)?;
        writeln!(f, "  内存音轨: {}", self.in_memory_track_count)?;
        writeln!(f, "  磁盘音轨: {}", self.on_disk_track_count)?;
        writeln!(f, "  按需加载音轨: {}", self.loaded_track_count)?;
        writeln!(
            f,
            "  基础内存: {:.2} MB",
            self.base_memory_bytes as f64 / 1024.0 / 1024.0
        )?;
        writeln!(
            f,
            "  按需内存: {:.2} MB",
            self.loaded_memory_bytes as f64 / 1024.0 / 1024.0
        )?;
        writeln!(
            f,
            "  总内存: {:.2} MB / {:.2} MB",
            self.total_memory_bytes as f64 / 1024.0 / 1024.0,
            self.memory_limit_bytes as f64 / 1024.0 / 1024.0
        )?;
        writeln!(f, "  总音符: {}", self.total_notes)?;
        writeln!(f, "  高力度音符(>1): {}", self.high_velocity_notes)?;
        Ok(())
    }
}
