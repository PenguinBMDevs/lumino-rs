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
/// - `Chunk`：全量会话数据块（携带 track_id，WGPU 侧据此构建音轨段表）
/// - `Done`：全量会话结束（finish + 清空段表）
/// - `TrackDelta`：单音轨整段替换（等长 = 音符级增量；变长 = GPU 内部搬移后续段）
#[derive(Debug)]
pub enum OnionSkinStreamMsg {
    /// 全量会话数据块：属于 `track_id` 音轨的实例（连续同轨块续写同一段）
    Chunk {
        track_id: usize,
        instances: Vec<crate::NoteInstance>,
    },
    /// 全量会话结束（WGPU 侧 finish_streaming_upload + 重置段表）
    Done,
    /// 单音轨增量替换：该音轨段整体替换为新内容
    TrackDelta {
        track_id: usize,
        instances: Vec<crate::NoteInstance>,
    },
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
        index: usize,
        instance: crate::NoteInstance,
    },
    /// 更新多个音符
    UpdateMany {
        start_index: usize,
        instances: Vec<crate::NoteInstance>,
    },
    /// 移除音符
    Remove(usize),
    /// 保序删除区间：删除 `[index, index+count)`，后续段 GPU 内部左移
    ///
    /// 主音轨事件级增量（卷帘编辑）专用——区别于 `Remove`（swap-remove 乱序），
    /// 保持 GPU buffer 顺序与 `notes` 索引一致。
    RemoveAt { index: usize, count: usize },
    /// 清空所有音符
    Clear,
}

// 公开导出
pub use types::GpuNoteBuffer;
