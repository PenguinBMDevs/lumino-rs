use super::params::RenderParams;
use crate::{TextureWaterfallConfig, WaterfallNote};

/// 高精度音轨重生成/脏覆层命令参数
///
/// 抽取出公共字段，避免在 `ControlCommand` 中重复定义，
/// 同时方便 UI 层构造命令。
#[derive(Debug, Clone)]
pub struct HiResTrackParams {
    /// 脏音轨索引（用于日志和确定 track_group）
    pub track_idx: u16,
    /// 该 track_group 内所有音轨的音符列表。
    /// 索引 0 对应 track_group 内的第一个音轨，即
    /// `track_group * WATERFALL_TRACKS_PER_GROUP`。
    /// 使用 Host 提供的最新音符数据重新合并 group tile，
    /// 避免读取可能过期的硬盘缓存导致同组其他音轨被覆盖为旧数据。
    pub group_notes: Vec<Vec<WaterfallNote>>,
    /// 需要生成覆层的脏 time_group 集合
    ///
    /// 仅 `ShowHiResDirtyOverlay` 命令使用：临时覆层只覆盖实际发生编辑的
    /// time_group，避免覆盖未编辑区域导致原贴图瀑布流被空白覆层盖住。
    /// `RegenerateHiResTrack` 命令传空 Vec（重生以全量替换覆层）。
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

impl HiResTrackParams {
    /// 创建新的高精度音轨参数
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

/// 视频帧数据发送器（包装 mpsc::Sender 以实现 Debug）
///
/// 渲染线程读回 BGRA 帧后通过此 sender 发送给 Runner 线程写入 FFmpeg。
#[derive(Clone)]
pub struct FrameSender(pub std::sync::mpsc::Sender<Vec<u8>>);

impl std::fmt::Debug for FrameSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameSender").finish()
    }
}

/// 控制命令
#[derive(Debug)]
pub enum ControlCommand {
    /// 调整窗口大小
    Resize { width: u32, height: u32 },
    /// 停止渲染线程
    Shutdown,
    /// 启动贴图瀑布流生成
    GenerateHiResOnionSkin {
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
    /// 释放高精度贴图瀑布流资源
    DisposeHiResOnionSkin,
    /// 重生成指定音轨组的贴图瀑布流（编辑后冷静期到期触发）
    RegenerateHiResTrack(HiResTrackParams),
    /// 显示编辑后的临时脏区域贴图覆层（切换音轨前立即触发）
    ShowHiResDirtyOverlay(HiResTrackParams),
    /// 启动视频导出：初始化 GPU→CPU 读回管线
    StartVideoExport {
        /// 视频宽度
        width: u32,
        /// 视频高度
        height: u32,
        /// 帧数据回传通道（渲染线程 → Runner）
        frame_tx: FrameSender,
        /// 帧缓冲区回收通道（ffmpeg 写入线程 → 渲染线程对象池）
        recycle_rx: std::sync::mpsc::Receiver<Vec<u8>>,
    },
    /// 渲染一帧视频并读回 BGRA 数据
    ///
    /// 渲染线程执行完整流程：离屏渲染 → copy 到 staging → submit → map_async → wait_read → 发送
    RenderVideoFrame {
        /// 帧渲染参数
        params: Box<RenderParams>,
    },
    /// 上传视频导出用贴图瀑布流（Runner 预生成后一次性传入）
    UploadHiResVideoTiles {
        /// 整合组贴图列表
        tiles: Vec<crate::WaterfallGroupTile>,
        /// 贴图瀑布流配置
        config: crate::TextureWaterfallConfig,
        /// 音轨总数
        track_count: u16,
        /// 键位数量（128 或 256）
        key_count: u16,
        /// 全曲总 tick
        total_ticks: u32,
        /// MIDI ppq
        ppq: u16,
    },
    /// 完成视频导出：释放读回管线资源
    FinishVideoExport,
}

impl ControlCommand {
    /// 构造 `RegenerateHiResTrack` 命令
    #[must_use]
    pub fn regenerate_track(params: HiResTrackParams) -> Self {
        Self::RegenerateHiResTrack(params)
    }

    /// 构造 `ShowHiResDirtyOverlay` 命令
    #[must_use]
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
