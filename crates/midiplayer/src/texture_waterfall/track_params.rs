//! 贴图瀑布流音轨重生成/脏覆层命令参数

use crate::texture_waterfall::config::TextureWaterfallConfig;
use crate::texture_waterfall::note::WaterfallNote;

/// 贴图瀑布流音轨重生成/脏覆层命令参数
///
/// 抽取出公共字段，避免在 [`crate::texture_waterfall::command::WaterfallCommand`]
/// 中重复定义，同时方便宿主 UI 层构造命令。
#[derive(Debug, Clone)]
pub struct WaterfallTrackParams {
    /// 脏音轨索引（用于日志和确定 track_group）
    pub track_idx: u16,
    /// 该 track_group 内所有音轨的音符列表。
    /// 索引 0 对应 track_group 内的第一个音轨，即
    /// `track_group * WATERFALL_TRACKS_PER_GROUP`。
    /// 使用宿主提供的最新音符数据重新合并 group tile，
    /// 避免读取可能过期的硬盘缓存导致同组其他音轨被覆盖为旧数据。
    pub group_notes: Vec<Vec<WaterfallNote>>,
    /// 需要生成覆层的脏 time_group 集合
    ///
    /// 仅 `ShowDirtyOverlay` 命令使用：临时覆层只覆盖实际发生编辑的
    /// time_group，避免覆盖未编辑区域导致原贴图被空白覆层盖住。
    /// `RegenerateTrack` 命令传空 Vec（重生以全量替换覆层）。
    pub dirty_time_groups: Vec<u32>,
    /// MIDI ppq
    pub ppq: u16,
    /// 键位数量
    pub key_count: u16,
    /// 全曲总 tick
    pub total_ticks: u32,
    /// 音轨总数（用于推断音轨组范围）
    pub track_count: u16,
    /// 贴图瀑布流配置
    pub config: TextureWaterfallConfig,
    /// MIDI 内容哈希（缓存分桶）
    pub midi_hash: String,
}

impl WaterfallTrackParams {
    /// 创建新的贴图瀑布流音轨参数
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        track_idx: u16,
        group_notes: Vec<Vec<WaterfallNote>>,
        dirty_time_groups: Vec<u32>,
        ppq: u16,
        key_count: u16,
        total_ticks: u32,
        track_count: u16,
        config: TextureWaterfallConfig,
        midi_hash: String,
    ) -> Self {
        Self {
            track_idx,
            group_notes,
            dirty_time_groups,
            ppq,
            key_count,
            total_ticks,
            track_count,
            config,
            midi_hash,
        }
    }
}
