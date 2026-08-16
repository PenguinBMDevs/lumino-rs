//! 贴图瀑布流控制命令（宿主 UI → 渲染线程 → 贴图瀑布流 runner）

use crate::texture_waterfall::config::TextureWaterfallConfig;
use crate::texture_waterfall::note::WaterfallNote;
use crate::texture_waterfall::track_params::WaterfallTrackParams;
use crate::texture_waterfall::types::WaterfallGroupTile;

/// 贴图瀑布流控制命令
#[derive(Debug)]
pub enum WaterfallCommand {
    /// 启动贴图瀑布流生成（后台线程流式生成并回传上传）
    Generate {
        /// 每轨音符列表
        notes: Vec<Vec<WaterfallNote>>,
        /// MIDI ppq
        ppq: u16,
        /// 键位数量（128 或 256，决定贴图高度）
        key_count: u16,
        /// 全曲总 tick
        total_ticks: u32,
        /// 贴图瀑布流配置
        config: TextureWaterfallConfig,
        /// MIDI 内容哈希（缓存分桶）
        midi_hash: String,
    },
    /// 释放贴图瀑布流资源（关闭 MIDI / 新建工程时调用）
    Dispose,
    /// 重生成指定音轨组的贴图（编辑后冷静期到期触发）
    RegenerateTrack(WaterfallTrackParams),
    /// 显示编辑后的临时脏区域贴图覆层（切换音轨前立即触发）
    ShowDirtyOverlay(WaterfallTrackParams),
    /// 上传视频导出预生成的贴图（Runner 预生成后一次性传入）
    UploadVideoTiles {
        /// 整合组贴图列表
        tiles: Vec<WaterfallGroupTile>,
        /// 贴图瀑布流配置
        config: TextureWaterfallConfig,
        /// 音轨总数
        track_count: u16,
        /// 键位数量（128 或 256）
        key_count: u16,
        /// 全曲总 tick
        total_ticks: u32,
        /// MIDI ppq
        ppq: u16,
    },
}

impl WaterfallCommand {
    /// 构造 `RegenerateTrack` 命令
    #[must_use]
    pub fn regenerate_track(params: WaterfallTrackParams) -> Self {
        Self::RegenerateTrack(params)
    }

    /// 构造 `ShowDirtyOverlay` 命令
    #[must_use]
    pub fn show_dirty_overlay(params: WaterfallTrackParams) -> Self {
        Self::ShowDirtyOverlay(params)
    }
}
