//! Root 内存占用快照收集

use crate::root::{MemoryBreakdown, Root};

impl Root {
    /// 收集各组件的内存占用快照
    pub fn memory_breakdown(&self) -> MemoryBreakdown {
        MemoryBreakdown {
            editor: self.editor.memory_breakdown(),
            ..Default::default()
        }
    }
}
