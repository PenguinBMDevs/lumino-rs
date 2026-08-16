//! 贴图瀑布流流式生成消息

/// 后台生成线程流式输出的消息
///
/// 按 time_group 串行推进：每个 track_group 的整合组贴图一生成完毕即单独发送，
/// 渲染线程收到后仅做 GPU 上传（DMA，非阻塞），上传完即可释放 CPU 像素缓冲。
/// 避免一个 time_group 的所有贴图在内存中累积后再统一上传。
///
/// 使用 `sync_channel(1)` 有界通道：channel 满时后台 send 阻塞，
/// 强制后台线程等待渲染线程消费——背压机制，防止无界积压导致 CPU 内存峰值。
#[derive(Debug)]
pub enum WaterfallStreamMsg {
    /// 某个 (track_group, time_group) 已合并的整合组像素缓冲（width × height × 4 字节）
    TimeGroupMerged {
        /// 音轨组索引
        track_group: u32,
        /// 时间组索引
        time_group: u32,
        /// RGBA8 像素缓冲
        pixels: Vec<u8>,
        /// 贴图宽度
        width: u32,
        /// 贴图高度
        height: u32,
    },
    /// 单组重生已完成，清理该 track_group 的临时脏区域覆层
    ///
    /// 仅 `RegenerateTrack` 路径发送，必须在所有 `TimeGroupMerged` 之后发送，
    /// 渲染线程按 FIFO 处理，确保新底贴图全部上传后才清理覆层。
    ClearDirtyOverlay(u32),
    /// 所有贴图生成完毕
    Finished,
}
