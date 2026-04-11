//! 音符实例缓冲区 - 高性能 GPU 渲染支持
//!
//! 优化策略：
//! 1. 直接存储 NoteInstance 而不是 Note，避免每次渲染时转换
//! 2. 使用双缓冲：CPU 端一个缓冲区，GPU 端一个缓冲区
//! 3. 增量更新：只上传新增或修改的音符
//! 4. 空间索引直接存储索引而不是引用，避免重建

use lumino_gfx::NoteInstance;
use iced_core::Color;

/// 音符实例条目（包含实例数据和脏标记）
#[derive(Debug, Clone)]
pub struct NoteInstanceEntry {
    /// 实例数据
    pub instance: NoteInstance,
    /// 是否需要重新上传到 GPU
    pub dirty: bool,
    /// 是否被删除
    pub deleted: bool,
}

impl NoteInstanceEntry {
    pub fn new(instance: NoteInstance) -> Self {
        Self {
            instance,
            dirty: true,
            deleted: false,
        }
    }
}

/// 音符实例缓冲区 - 管理所有音符的 GPU 实例数据
pub struct NoteInstanceBuffer {
    /// 所有音符实例（按索引存储）
    entries: Vec<NoteInstanceEntry>,
    /// 空闲索引池（用于复用）
    free_indices: Vec<usize>,
    /// 脏标记（是否有数据需要上传）
    has_dirty: bool,
    /// 当前活动音符数量
    active_count: usize,
}

impl NoteInstanceBuffer {
    pub fn new() -> Self {
        Self {
            entries: Vec::with_capacity(1024),
            free_indices: Vec::new(),
            has_dirty: false,
            active_count: 0,
        }
    }

    /// 添加一个音符实例，返回其索引
    pub fn add(&mut self, instance: NoteInstance) -> usize {
        self.has_dirty = true;
        
        // 优先复用空闲索引
        if let Some(index) = self.free_indices.pop() {
            self.entries[index] = NoteInstanceEntry::new(instance);
            self.active_count += 1;
            index
        } else {
            let index = self.entries.len();
            self.entries.push(NoteInstanceEntry::new(instance));
            self.active_count += 1;
            index
        }
    }

    /// 更新指定索引的音符实例
    pub fn update(&mut self, index: usize, instance: NoteInstance) {
        if index < self.entries.len() && !self.entries[index].deleted {
            self.entries[index].instance = instance;
            self.entries[index].dirty = true;
            self.has_dirty = true;
        }
    }

    /// 删除指定索引的音符实例
    pub fn remove(&mut self, index: usize) {
        if index < self.entries.len() && !self.entries[index].deleted {
            self.entries[index].deleted = true;
            self.entries[index].dirty = true;
            self.free_indices.push(index);
            self.active_count -= 1;
            self.has_dirty = true;
        }
    }

    /// 获取所有活动的实例（用于上传到 GPU）
    pub fn get_active_instances(&self) -> Vec<NoteInstance> {
        self.entries
            .iter()
            .filter(|e| !e.deleted)
            .map(|e| e.instance)
            .collect()
    }

    /// 获取所有脏实例的索引和引用（用于增量上传）
    pub fn get_dirty_instances(&self) -> Vec<(usize, &NoteInstance)> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.dirty && !e.deleted)
            .map(|(i, e)| (i, &e.instance))
            .collect()
    }

    /// 清除所有脏标记
    pub fn clear_dirty(&mut self) {
        for entry in &mut self.entries {
            entry.dirty = false;
        }
        self.has_dirty = false;
    }

    /// 是否有脏数据需要上传
    pub fn has_dirty(&self) -> bool {
        self.has_dirty
    }

    /// 获取活动音符数量
    pub fn len(&self) -> usize {
        self.active_count
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.active_count == 0
    }

    /// 清空所有数据
    pub fn clear(&mut self) {
        self.entries.clear();
        self.free_indices.clear();
        self.has_dirty = true;
        self.active_count = 0;
    }

    /// 批量添加音符实例
    pub fn extend(&mut self, instances: impl Iterator<Item = NoteInstance>) {
        for instance in instances {
            self.add(instance);
        }
    }
}

impl Default for NoteInstanceBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_get() {
        let mut buffer = NoteInstanceBuffer::new();
        let instance = NoteInstance::new(0.0, 60.0, 100.0, [1.0, 0.0, 0.0, 1.0]);
        
        let index = buffer.add(instance);
        assert_eq!(index, 0);
        assert_eq!(buffer.len(), 1);
        assert!(buffer.has_dirty());
    }

    #[test]
    fn test_remove_and_reuse() {
        let mut buffer = NoteInstanceBuffer::new();
        let instance1 = NoteInstance::new(0.0, 60.0, 100.0, [1.0, 0.0, 0.0, 1.0]);
        let instance2 = NoteInstance::new(100.0, 62.0, 100.0, [0.0, 1.0, 0.0, 1.0]);
        
        let idx1 = buffer.add(instance1);
        let _idx2 = buffer.add(instance2.clone());
        
        buffer.remove(idx1);
        assert_eq!(buffer.len(), 1);
        
        // 应该复用 idx1
        let idx3 = buffer.add(instance2);
        assert_eq!(idx3, idx1);
        assert_eq!(buffer.len(), 2);
    }

    #[test]
    fn test_get_active_instances() {
        let mut buffer = NoteInstanceBuffer::new();
        
        for i in 0..100 {
            let instance = NoteInstance::new(i as f32 * 10.0, 60.0, 100.0, [1.0, 0.0, 0.0, 1.0]);
            buffer.add(instance);
        }
        
        let active = buffer.get_active_instances();
        assert_eq!(active.len(), 100);
    }

    #[test]
    fn test_clear_dirty() {
        let mut buffer = NoteInstanceBuffer::new();
        let instance = NoteInstance::new(0.0, 60.0, 100.0, [1.0, 0.0, 0.0, 1.0]);
        
        buffer.add(instance);
        assert!(buffer.has_dirty());
        
        buffer.clear_dirty();
        assert!(!buffer.has_dirty());
    }
}
