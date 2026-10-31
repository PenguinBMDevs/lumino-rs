//! 洋葱皮后端会话锁状态（「加载 MIDI 后禁止切换后端」的判定真源）
//!
//! # 为什么不能用 `document.is_some()` 当锁
//!
//! 启动、新建工程、关闭工程三条路径都会调用 `Host::init_blank_project()`，它会建一个
//! 2 轨空白 `MidiDocument` 并挂到 `EditorData.document`（`document == Some(..)`）。
//! 若把「有文档」当成「已加载 MIDI」，会话锁会从**第一帧**起永久为真：
//! 设置页的洋葱皮后端下拉恒锁定，用户在空白工程里既选不了 CPU 也切不回 GPU——
//! 「加载 MIDI 后禁止切换」的硬约束会退化成「永远不能切换」，功能等于不可用。
//!
//! 用户约束的原话是「**加载 MIDI 后**禁止切换」，空白工程不是加载 MIDI。
//! 因此锁只由**文档来源**决定，而不是「文档是否存在」：
//!
//! | 事件 | 调用点 | 结果 |
//! |---|---|---|
//! | 真实 MIDI / 工程文件载入 | `midi_handler::import_midi_to_editor`（MIDI、LMPJ、云、保存后重载） | 锁定 |
//! | 导出前自动保存重载 | `runner::menu::file::export` | 锁定 |
//! | 启动 / 新建 / 关闭工程 | `Host::init_blank_project`（`clear_editor` 尾调用） | 解锁 |
//!
//! 上表两条「载入」路径都只经过 `Host::set_midi_document` 这一个入口，
//! 因此状态搬迁点只有两处：`set_midi_document`（锁定）与 `init_blank_project`（解锁）。
//!
//! 判定与副作用分离：本类型**只存状态**，`Host` 负责在正确调用点搬动它。
//! 设置面板的锁注入（`set_settings_onion_backend_locked`）与落盘守卫
//! （`runner::inner::persist`）读的都是同一个值（经 `Host::onion_backend_locked()`），
//! 不存在第二套判断。

/// MIDI 载入会话锁状态。
///
/// 默认**未锁定**：应用启动时先是空白工程，用户可以先选后端再决定加载哪个 MIDI。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct MidiLoadLock {
    /// 是否已加载**真实** MIDI/工程（空白工程恒为 `false`）
    loaded: bool,
}

impl MidiLoadLock {
    /// 真实 MIDI / 工程文件载入：进入锁定态（此后禁止切换洋葱皮后端）。
    pub(crate) fn mark_loaded(&mut self) {
        self.loaded = true;
    }

    /// 空白工程（启动 / 新建 / 关闭工程）：回到未加载态，允许再次选择后端。
    pub(crate) fn mark_blank(&mut self) {
        self.loaded = false;
    }

    /// 是否已加载真实 MIDI（= 会话锁是否生效）。
    pub(crate) fn is_loaded(self) -> bool {
        self.loaded
    }
}

#[cfg(test)]
mod tests {
    use super::MidiLoadLock;

    /// 启动即空白工程：锁必须是**解锁**态，否则设置页后端下拉永远不可用。
    #[test]
    fn test_default_is_unlocked() {
        assert!(
            !MidiLoadLock::default().is_loaded(),
            "默认（空白工程）必须允许选择洋葱皮后端"
        );
    }

    /// 真实 MIDI 载入后锁定：这是用户「加载 MIDI 后禁止切换」硬约束的判定。
    #[test]
    fn test_mark_loaded_locks() {
        let mut lock = MidiLoadLock::default();
        lock.mark_loaded();
        assert!(lock.is_loaded(), "加载真实 MIDI 后必须锁定");
    }

    /// 新建/关闭工程回到空白：必须解锁，否则换工程后再也切不了后端。
    #[test]
    fn test_mark_blank_unlocks_after_load() {
        let mut lock = MidiLoadLock::default();
        lock.mark_loaded();
        lock.mark_blank();
        assert!(
            !lock.is_loaded(),
            "新建/关闭工程（空白工程）后必须解锁：空白工程不是「加载 MIDI」"
        );
    }

    /// 两个迁移动作都幂等：重复调用不会在锁定/解锁之间抖动。
    #[test]
    fn test_transitions_are_idempotent() {
        let mut lock = MidiLoadLock::default();
        lock.mark_blank();
        assert!(!lock.is_loaded());
        lock.mark_loaded();
        lock.mark_loaded();
        assert!(lock.is_loaded());
        lock.mark_blank();
        lock.mark_blank();
        assert!(!lock.is_loaded());
    }
}
