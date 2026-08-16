//! 贴图瀑布流元数据

/// 贴图瀑布流元数据（无像素数据，用于视口计算）
#[derive(Clone, Debug)]
pub struct WaterfallMeta {
    /// 音轨总数
    pub track_count: u16,
    /// 音轨组数（= ceil(track_count / WATERFALL_TRACKS_PER_GROUP)），
    /// 用于判断 time_group 贴图是否收齐
    pub track_groups: u32,
    /// 键位数量（128 或 256，决定贴图高度）
    pub key_count: u16,
    /// 时间组总数
    pub time_groups: u32,
    /// 每个时间组的 tick 数
    pub ticks_per_group: u32,
}
