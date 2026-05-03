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
    /// 清空所有音符
    Clear,
}

// 公开导出
pub use types::GpuNoteBuffer;
