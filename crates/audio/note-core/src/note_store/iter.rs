//! NoteStore 迭代器与单音符修改视图
//!
//! - `NoteMut`：单音符修改句柄，避免中间 Note 结构体
//! - `NoteStoreIter`：返回 `Note` 副本的迭代器
//! - `NoteStoreRefIter`：返回 `NoteView`（Copy 语义）的迭代器，零 clone

use super::{Chunk, NoteStore, NoteView};
use crate::note::Note;

/// 单音符修改视图
///
/// 通过 `NoteStore::get_mut` 获取，提供字段级别的 getter/setter，
/// 修改直接写入底层 SoA 数组，无中间 Note 副本。
pub struct NoteMut<'a> {
    pub(crate) chunk: &'a mut Chunk,
    pub(crate) local_idx: usize,
}

impl<'a> NoteMut<'a> {
    pub fn tick(&self) -> f32 {
        self.chunk.ticks[self.local_idx]
    }
    pub fn key(&self) -> u16 {
        self.chunk.keys[self.local_idx]
    }
    pub fn length(&self) -> f32 {
        self.chunk.lengths[self.local_idx]
    }
    pub fn velocity(&self) -> u8 {
        self.chunk.velocities[self.local_idx]
    }
    pub fn channel(&self) -> u8 {
        self.chunk.channels[self.local_idx]
    }

    pub fn set_tick(&mut self, v: f32) {
        self.chunk.ticks[self.local_idx] = v;
    }
    pub fn set_key(&mut self, v: u16) {
        self.chunk.keys[self.local_idx] = v;
    }
    pub fn set_length(&mut self, v: f32) {
        self.chunk.lengths[self.local_idx] = v;
    }
    pub fn set_velocity(&mut self, v: u8) {
        self.chunk.velocities[self.local_idx] = v;
    }
    pub fn set_channel(&mut self, v: u8) {
        self.chunk.channels[self.local_idx] = v;
    }

    /// 转换为 Note 副本
    pub fn to_note(&self) -> Note {
        Note::from_raw(
            self.chunk.ticks[self.local_idx],
            self.chunk.keys[self.local_idx],
            self.chunk.lengths[self.local_idx],
            self.chunk.velocities[self.local_idx],
            self.chunk.channels[self.local_idx],
        )
    }
}

/// Note 副本迭代器（每个音符 clone 一次）
pub struct NoteStoreIter<'a> {
    pub(crate) store: &'a NoteStore,
    pub(crate) idx: usize,
}

impl<'a> Iterator for NoteStoreIter<'a> {
    type Item = Note;

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx >= self.store.total_len {
            return None;
        }
        let note = self.store.get(self.idx);
        self.idx += 1;
        note
    }
}

impl<'a> ExactSizeIterator for NoteStoreIter<'a> {
    fn len(&self) -> usize {
        self.store.total_len - self.idx
    }
}

/// NoteView 迭代器（Copy 语义，零 clone）
///
/// 16M 音符场景下比 `NoteStoreIter` 节省 ~80ms 的 Note 结构体构造开销。
pub struct NoteStoreRefIter<'a> {
    pub(crate) store: &'a NoteStore,
    pub(crate) idx: usize,
}

impl<'a> Iterator for NoteStoreRefIter<'a> {
    type Item = NoteView;

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx >= self.store.total_len {
            return None;
        }
        let view = self.store.get_ref(self.idx);
        self.idx += 1;
        view
    }
}

impl<'a> ExactSizeIterator for NoteStoreRefIter<'a> {
    fn len(&self) -> usize {
        self.store.total_len - self.idx
    }
}
