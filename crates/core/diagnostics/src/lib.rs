//! Lumino 运行时诊断与内存治理
//!
//! 合并原 `lumino-memory-monitor` 与 `lumino-memtrace` 的共享运行时诊断设施。

pub mod memory_monitor;
/// 带分配标签的全局分配器与内存快照
pub mod memtrace;

// 重新导出常用类型，保持调用方路径简短
pub use memory_monitor::{MemoryMonitor, spawn_monitor_thread};
pub use memtrace::{
    AllocTag, Snapshot, TaggedAlloc, add_gpu_resource, gpu_resource_bytes, gpu_resource_mb,
    purge_free_pages, sub_gpu_resource, with_tag,
};
