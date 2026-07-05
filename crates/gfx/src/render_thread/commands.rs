use super::params::RenderParams;
use crate::{HiResConfig, OnionSkinNote};

/// 高精度洋葱皮音轨参数（`RegenerateHiResTrack` / `ShowHiResDirtyOverlay` 共享字段）
///
/// 将两个枚举变体的 7 个相同字段抽取为独立结构体，
/// 消除 `host.rs` 中两个发送函数的参数重复。
#[derive(Debug, Clone)]
pub struct HiResTrackParams {
    /// 脏音轨索引（用于日志和确定 track_group）
    pub track_idx: u16,
    /// 该 track_group 内所有音轨的音符列表。
    /// 索引 0 对应该 track_group 的第一个音轨。
    pub group_notes: Vec<Vec<OnionSkinNote>>,
    /// MIDI ppq
    pub ppq: u16,
    /// 键位数量（128 或 256）
    pub key_count: u16,
    /// 全曲总 tick
    pub total_ticks: u32,
    /// 音轨总数（用于推断音轨组范围）
    pub track_count: u16,
    /// 高精度贴图配置
    pub config: HiResConfig,
    /// MIDI 内容哈希（缓存分桶）
    pub midi_hash: String,
}

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
    RegenerateHiResTrack(HiResTrackParams),
    /// 显示编辑后的临时脏区域贴图覆层（切换音轨前立即触发）
    ShowHiResDirtyOverlay(HiResTrackParams),
}

impl ControlCommand {
    /// 创建重生成指定音轨组命令
    pub fn regenerate_track(params: HiResTrackParams) -> Self {
        Self::RegenerateHiResTrack(params)
    }

    /// 创建显示脏区域覆层命令
    pub fn show_dirty_overlay(params: HiResTrackParams) -> Self {
        Self::ShowHiResDirtyOverlay(params)
    }
}

/// 渲染命令（UI 线程 -> 渲染线程）
#[derive(Debug)]
pub enum RenderCommand {
    /// 渲染一帧
    Render(Box<RenderParams>),
    /// 控制命令
    Control(ControlCommand),
}
