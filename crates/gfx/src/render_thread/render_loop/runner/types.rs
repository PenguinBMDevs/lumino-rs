/// 高精度贴图元数据（无像素数据，用于视口计算）
#[allow(dead_code)]
pub(crate) struct HiResMeta {
    pub(crate) track_count: u16,
    /// 音轨组数（= ceil(track_count / TRACKS_PER_GROUP)），用于判断 time_group 贴图是否收齐
    pub(crate) track_groups: u32,
    pub(crate) key_count: u16,
    pub(crate) time_groups: u32,
    pub(crate) ticks_per_group: u32,
}

/// 后台生成线程流式输出的消息
///
/// 按 time_group 串行推进：每个 track_group 的整合组贴图一生成完毕即单独发送，
/// 渲染线程收到后仅做 GPU 上传（DMA，非阻塞），上传完即可释放 CPU 像素缓冲。
/// 避免一个 time_group 的所有贴图在内存中累积后再统一上传。
///
/// 使用 `sync_channel(1)` 有界通道：channel 满时后台 send 阻塞，
/// 强制后台线程等待渲染线程消费——背压机制，防止无界积压导致 CPU 内存峰值。
pub(crate) enum HiResStreamMsg {
    /// 某个 (track_group, time_group) 已合并的整合组像素缓冲（width × height × 4 字节）
    TimeGroupMerged {
        track_group: u32,
        time_group: u32,
        pixels: Vec<u8>,
        width: u32,
        height: u32,
    },
    /// 所有贴图生成完毕
    Finished,
}
