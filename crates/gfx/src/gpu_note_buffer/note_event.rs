//! 音符编辑事件

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
