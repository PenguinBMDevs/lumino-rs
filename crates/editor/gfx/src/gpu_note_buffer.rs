//! GPU 音符缓冲区 - 音符数据常驻 GPU 内存
//!
//! 架构说明：
//! - 音符数据上传一次到 GPU，之后常驻 GPU 内存
//! - 只支持增量更新（添加/修改/删除单个音符）
//! - 视口变化时只更新 camera uniform，不重新上传所有数据
//! - 严格控制 GPU 内存占用，支持动态扩容/缩容

// 子模块定义
pub mod internal;
pub mod ops;
pub mod types;

/// 洋葱皮流式 / 增量上传消息（UI 线程 → WGPU 渲染线程）
///
/// 事件级增量优化（2026-08-05）：黑乐谱（单轨海量音符）场景下，
/// 旧协议只能整轨全量重传。新协议携带音轨边界 + 单音轨增量替换：
/// - `Reserve`：全量会话前预分配容量（2026-08-06：消除流式 append 的 2×
///   倍增余量，2.9 亿音符节省 ~4GB GPU 显存）
/// - `Chunk`：全量会话数据块（携带 track_id，WGPU 侧据此构建音轨段表）
/// - `Done`：全量会话结束（finish + 清空段表）
/// - `TrackDelta`：单音轨整段替换（等长 = 音符级增量；变长 = GPU 内部搬移后续段）
/// - `SetViewState`：切轨/静音变化（2026-08-06 统一全量渲染：只更新 ViewState
///   uniform，GPU 音符数据零重传）
/// - `PreviewInstances`：预览音符（Drawing/hover/i2m）实例替换（独立预览渲染器）
#[derive(Debug)]
pub enum OnionSkinStreamMsg {
    /// 全量会话前预分配实例容量（避免流式 append 2× 倍增的容量余量）
    Reserve {
        /// 预分配的实例总容量
        total: usize,
    },
    /// 全量会话数据块：属于 `track_id` 音轨的实例（连续同轨块续写同一段）
    Chunk {
        /// 本数据块所属的音轨 id
        track_id: usize,
        /// 该音轨段的音符实例列表
        instances: Vec<crate::NoteInstance>,
    },
    /// 全量会话结束（WGPU 侧 finish_streaming_upload + 重置段表）
    Done,
    /// 单音轨增量替换：该音轨段整体替换为新内容
    TrackDelta {
        /// 被替换的音轨 id
        track_id: usize,
        /// 该音轨段的新实例列表
        instances: Vec<crate::NoteInstance>,
    },
    /// 视图状态更新：当前音轨（track_idx+1）+ 静音音轨集合
    ///
    /// 统一全量渲染：主音轨 = 全量 buffer 中 `current_track` 段，切轨/静音
    /// 只更新 uniform（shader 动态主轨着色/静音隐藏），**零数据重传**。
    SetViewState {
        /// 当前主音轨编码：track_idx + 1（0 = 无主音轨）
        current_track: u32,
        /// 静音音轨 id 集合
        muted_tracks: Vec<usize>,
    },
    /// 预览音符实例替换（Drawing / hover / i2m 预览，独立预览渲染器）
    ///
    /// 预览音符不在 document 中、不进全量 buffer；每次变化整体替换
    /// （预览量小，<1K 实例，全量替换开销可忽略）。
    PreviewInstances(Vec<crate::NoteInstance>),
}

/// 音符编辑事件
#[derive(Debug, Clone)]
pub enum NoteEvent {
    /// 重新加载所有音符
    Reset(Vec<crate::NoteInstance>),
    /// 添加音符
    Add(crate::NoteInstance),
    /// 更新单个音符
    Update {
        /// 目标音符在 buffer 中的索引
        index: usize,
        /// 替换后的音符实例
        instance: crate::NoteInstance,
    },
    /// 更新多个音符
    UpdateMany {
        /// 起始索引（连续区间起点）
        start_index: usize,
        /// 新的音符实例列表（与区间等长）
        instances: Vec<crate::NoteInstance>,
    },
    /// 移除音符
    Remove(usize),
    /// 保序删除区间：删除 `[index, index+count)`，后续段 GPU 内部左移
    ///
    /// 主音轨事件级增量（卷帘编辑）专用——区别于 `Remove`（swap-remove 乱序），
    /// 保持 GPU buffer 顺序与 `notes` 索引一致。
    RemoveAt {
        /// 删除区间的起始索引
        index: usize,
        /// 删除的连续数量
        count: usize,
    },
    /// 保序插入区间：在 `index` 处插入 `instances`，后续段 GPU 内部右移
    ///
    /// 主音轨可见列表 diff 增量（切轨/增删/undo 兜底）专用——与 `RemoveAt`
    /// 互为逆操作，保持 GPU buffer 顺序与可见音符列表一致。
    Insert {
        /// 插入位置的索引
        index: usize,
        /// 待插入的音符实例列表
        instances: Vec<crate::NoteInstance>,
    },
    /// 清空所有音符
    Clear,
}

// 公开导出
pub use types::GpuNoteBuffer;
