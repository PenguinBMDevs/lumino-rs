use super::params::RenderParams;
use crate::{WaterfallCommand, WaterfallTrackParams};

/// 单个视频帧消息（帧数据 + 可选钢琴卷帘键色）。
///
/// `key_colors` 由渲染线程的 GPU 活跃键内核推导（128 键 × RGBA = 1024B），
/// 仅钢琴卷帘导出填充；瀑布流/MIDITrail/CPU 渲染模式为 `None`。
/// 与帧数据同通道发送，保证「键色 ↔ 帧」严格同序，避免错帧高亮。
#[derive(Debug)]
pub struct VideoFrameMessage {
    /// BGRA/RGBA 帧数据。
    pub data: Vec<u8>,
    /// 128 键 × RGBA 键色（与主 crate `keyboard::KEY_COLOR_BYTES` 同布局）。
    pub key_colors: Option<[u8; 1024]>,
}

/// 视频帧数据发送器（包装 mpsc::Sender 以实现 Debug）
///
/// 渲染线程读回 BGRA 帧后通过此 sender 发送给 Runner 线程写入 FFmpeg。
#[derive(Clone)]
pub struct FrameSender(pub std::sync::mpsc::Sender<VideoFrameMessage>);

impl std::fmt::Debug for FrameSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameSender").finish()
    }
}

/// 控制命令
#[derive(Debug)]
pub enum ControlCommand {
    /// 调整窗口大小
    Resize {
        /// 新的窗口宽度（像素）
        width: u32,
        /// 新的窗口高度（像素）
        height: u32,
    },
    /// 停止渲染线程
    Shutdown,
    /// 贴图瀑布流控制命令（生成/释放/重生成/脏覆层/视频上传）
    ///
    /// 命令定义与处理逻辑位于 lumino-midiplayer，本变体仅做转发。
    Waterfall(WaterfallCommand),
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
    /// 完成视频导出：释放读回管线资源
    FinishVideoExport,
}

impl ControlCommand {
    /// 构造贴图瀑布流音轨重生成命令
    #[must_use]
    pub fn regenerate_track(params: WaterfallTrackParams) -> Self {
        Self::Waterfall(WaterfallCommand::regenerate_track(params))
    }

    /// 构造贴图瀑布流脏区域覆层命令
    #[must_use]
    pub fn show_dirty_overlay(params: WaterfallTrackParams) -> Self {
        Self::Waterfall(WaterfallCommand::show_dirty_overlay(params))
    }
}

/// 渲染命令（UI 线程 -> 渲染线程）
#[derive(Debug)]
pub enum RenderCommand {
    /// 渲染一帧
    ///
    /// `frame_id` 由 UI 线程在 `send_params` 时递增分配，渲染线程渲染完成后
    /// 写入 `rendered_frame` 并 notify。UI 线程在 present（copy 到 Surface）前
    /// `wait_for_frame(frame_id)`，避免拷到"尚未被渲染线程处理"的旧离屏帧
    /// （音符放置后不立即显示的竞态根因：UI 拷贝与 wgpu 渲染对共享离屏纹理的无同步竞态）。
    Render {
        params: Box<RenderParams>,
        frame_id: u64,
    },
    /// 控制命令
    Control(ControlCommand),
}
