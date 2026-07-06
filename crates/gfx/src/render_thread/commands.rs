use super::params::RenderParams;
use crate::{HiResConfig, OnionSkinNote};

/// 控制命令
#[derive(Debug)]
pub enum ControlCommand {
    /// 调整窗口大小
    Resize { width: u32, height: u32 },
    /// 停止渲染线程
    Shutdown,
    /// 启动高精度洋葱皮贴图生成
    GenerateHiResOnionSkin {
        /// 每轨音符列表
        notes: Vec<Vec<OnionSkinNote>>,
        /// MIDI ppq
        ppq: u16,
        /// 键位数量（128 或 256，决定贴图高度）
        key_count: u16,
        /// 全曲总 tick
        total_ticks: u32,
        /// 高精度贴图配置
        config: HiResConfig,
        /// MIDI 内容哈希（缓存分桶）
        midi_hash: String,
    },
    /// 释放高精度洋葱皮资源
    DisposeHiResOnionSkin,
    /// 重生成指定音轨组的高精度贴图（编辑后冷静期到期触发）
    RegenerateHiResTrack {
        /// 脏音轨索引（用于日志和确定 track_group）
        track_idx: u16,
        /// 该 track_group 内所有音轨的音符列表。
        /// 索引 0 对应 track_group 内的第一个音轨，即
        /// `track_group * TRACKS_PER_GROUP`。
        /// 使用 Host 提供的最新音符数据重新合并 group tile，
        /// 避免读取可能过期的硬盘缓存导致同组其他音轨被覆盖为旧数据。
        group_notes: Vec<Vec<OnionSkinNote>>,
        /// MIDI ppq
        ppq: u16,
        /// 键位数量
        key_count: u16,
        /// 全曲总 tick
        total_ticks: u32,
        /// 音轨总数（用于推断音轨组范围）
        track_count: u16,
        /// 高精度贴图配置
        config: HiResConfig,
        /// MIDI 内容哈希（缓存分桶）
        midi_hash: String,
    },
    /// 显示编辑后的临时脏区域贴图覆层（切换音轨前立即触发）
    ShowHiResDirtyOverlay {
        /// 脏音轨索引（用于日志和确定 track_group）
        track_idx: u16,
        /// 该 track_group 内所有音轨的音符列表。
        /// 索引 0 对应 track_group 内的第一个音轨。
        /// 合并为整合组贴图覆层，避免同组多个脏音轨互相覆盖。
        group_notes: Vec<Vec<OnionSkinNote>>,
        /// MIDI ppq
        ppq: u16,
        /// 键位数量
        key_count: u16,
        /// 全曲总 tick
        total_ticks: u32,
        /// 音轨总数（用于推断音轨组范围）
        track_count: u16,
        /// 高精度贴图配置
        config: HiResConfig,
        /// MIDI 内容哈希（缓存分桶）
        midi_hash: String,
    },
}

/// 渲染命令（UI 线程 -> 渲染线程）
#[derive(Debug)]
pub enum RenderCommand {
    /// 渲染一帧
    Render(Box<RenderParams>),
    /// 控制命令
    Control(ControlCommand),
}
