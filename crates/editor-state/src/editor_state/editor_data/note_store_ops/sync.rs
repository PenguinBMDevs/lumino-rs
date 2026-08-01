//! NoteStore 同步操作：启用/禁用/回写 + BitVec→BitSet 转换工具

use bit_vec::BitVec;

use super::super::EditorData;
use super::super::NOTE_STORE_THRESHOLD;
use lumino_note_core::note_store::BitSet;

/// 将 `bit_vec::BitVec` 转换为 `NoteStore::BitSet`
///
/// **块级优化**：用 `blocks()` 获取 u64 块，跳过全 0 块 + `trailing_zeros` 只遍历选中位。
/// 16M 50% 选中 ~12ms（vs 旧实现逐位迭代 ~50ms）。
/// 16M 1% 选中 ~0.01ms（vs 旧实现 ~1ms）。
pub(super) fn bitvec_to_bitset(bv: &BitVec) -> BitSet {
    let len = bv.len();
    let mut selected_bits = BitSet::new(len);
    for (block_idx, block) in bv.blocks().enumerate() {
        if block == 0 {
            continue;
        }
        let base = block_idx * 64;
        let mut bits = block;
        while bits != 0 {
            let tz = bits.trailing_zeros() as usize;
            let idx = base + tz;
            if idx < len {
                selected_bits.set(idx);
            }
            bits &= bits - 1;
        }
    }
    selected_bits
}

impl EditorData {
    /// 同步 notes → note_store（从 im::Vector 重建 SoA 存储）
    ///
    /// 当音符数超过阈值时自动启用 NoteStore。调用时机：
    /// - MIDI 文件加载后
    /// - 音轨切换后（如果 note_store 已启用）
    /// - 批量操作后保持一致性
    pub fn sync_note_store(&mut self) {
        let count = self.notes.len();
        if count >= NOTE_STORE_THRESHOLD {
            if !self.note_store_enabled {
                tracing::info!(
                    "NoteStore 启用: {} 音符 ≥ 阈值 {}",
                    count,
                    NOTE_STORE_THRESHOLD
                );
            }
            self.note_store = lumino_note_core::note_store::NoteStore::from_im_vector(&self.notes);
            self.note_store_enabled = true;
        } else if self.note_store_enabled {
            self.note_store.clear();
            self.note_store_enabled = false;
            tracing::debug!(
                "NoteStore 禁用: {} 音符 < 阈值 {}",
                count,
                NOTE_STORE_THRESHOLD
            );
        }
    }

    /// 从 note_store 回写到 notes（批量操作后恢复一致性）
    ///
    /// 回写前先 compact() 移除墓碑标记的音符，确保 notes 与 store 一致。
    ///
    /// **优化**：无墓碑时跳过 compact()（O(N) 物理复制），只做 to_im_vector() 顺序扫描。
    pub fn sync_notes_from_store(&mut self) {
        if !self.note_store_enabled {
            return;
        }
        if self.note_store.has_tombstones() {
            self.note_store.compact();
        }
        self.notes = self.note_store.to_im_vector();
    }
}
