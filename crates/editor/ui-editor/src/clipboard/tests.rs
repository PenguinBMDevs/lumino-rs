//! 钢琴卷帘剪贴板闸门单元测试（Domino 互粘开关）
//!
//! 覆盖验收口径：
//! - 开关默认关闭（旧配置升级后同样落关闭）；
//! - 关闭时复制不构建 Domino 载荷、只写 Lumino 二进制，粘贴不消费 Domino 格式；
//! - 开启时行为与改动前一致（一次会话双格式，粘贴可解 Domino）。
//!
//! 与系统剪贴板相关的用例全部集中在 `#[cfg(windows)] mod clipboard_io` 内：
//! 一是 Domino 通路整体只在 Windows 存在，二是避免非 Windows 编译出现
//! `unused` 告警（CI 为 `clippy --all-targets -- -D warnings`）。
//!
//! 系统剪贴板是**进程外共享资源**（`OpenClipboard` 可能被其他进程短暂占用），
//! 因此本模块的读写一律：① 模块内互斥，避免自身用例互相覆盖；
//! ② 带退避重试直到**正向前置**成立，再断言负向结论——保证"没写 Domino"
//! 不会被误判成"根本没写进剪贴板"。

use crate::Editor;

/// Domino 互粘默认关闭：新装用户不该为用不到的格式付费
#[test]
fn test_domino_clipboard_disabled_by_default() {
    let editor = Editor::new();
    assert!(!editor.domino_clipboard_enabled(), "Domino 互粘默认应关闭");
}

/// 开关读写往返（Editor 侧字段，由 Root 设置链路注入）
#[test]
fn test_set_domino_clipboard_enabled_roundtrip() {
    let mut editor = Editor::new();
    editor.set_domino_clipboard_enabled(true);
    assert!(editor.domino_clipboard_enabled(), "开启后应读回 true");
    editor.set_domino_clipboard_enabled(false);
    assert!(!editor.domino_clipboard_enabled(), "关闭后应读回 false");
}

#[cfg(windows)]
mod clipboard_io {
    use std::sync::{Mutex, MutexGuard};

    use crate::Editor;
    use crate::clipboard::sys;
    use crate::note::Note;
    use crate::tests::test_helpers::seed_notes;

    /// 本模块用例串行闸：剪贴板是进程级单例，并发用例会互相覆盖
    static CLIPBOARD_LOCK: Mutex<()> = Mutex::new(());

    fn clipboard_guard() -> MutexGuard<'static, ()> {
        // 前一个用例 panic 会毒化锁，但剪贴板状态本就是"用完即弃"，
        // 直接取回内部值，避免一个失败连锁毒化其余用例
        CLIPBOARD_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 退避重试次数：5 次仍不满足即判失败，真实回归不会被重试吞掉
    const RETRY_ATTEMPTS: usize = 5;

    fn retry_sleep(attempt: usize) {
        std::thread::sleep(std::time::Duration::from_millis(20 * (attempt as u64 + 1)));
    }

    /// 当前轨 8 个音符并全部选中（`seed_notes` 按 start_tick 升序写入 document）
    fn seeded_editor() -> Editor {
        let mut editor = Editor::new();
        let notes: Vec<Note> = (0..8)
            .map(|i| Note::from_raw(i as f32 * 480.0, 60 + i as u16, 240.0, 100, 0))
            .collect();
        seed_notes(&mut editor, 1, 0, &notes);
        for i in 0..notes.len() {
            editor.editor_state.interaction.selected_notes.insert(i);
        }
        editor
    }

    /// 反复复制直到 `probe` 观测到预期的**正向**结果（容忍剪贴板偶发被占用）
    fn copy_until<T>(editor: &mut Editor, mut probe: impl FnMut() -> Option<T>) -> Option<T> {
        for attempt in 0..RETRY_ATTEMPTS {
            let _ = editor.copy_selected_notes_to_clipboard();
            if let Some(v) = probe() {
                return Some(v);
            }
            retry_sleep(attempt);
        }
        None
    }

    /// 反复复制直到 `probe` 同时给出**两个**正向观测（用于双格式同会话断言）
    fn copy_until_both(editor: &mut Editor) -> Option<(Vec<u8>, Vec<u8>)> {
        for attempt in 0..RETRY_ATTEMPTS {
            let _ = editor.copy_selected_notes_to_clipboard();
            if let (Some(b), Some(d)) = (sys::get_clipboard_binary(), sys::get_clipboard_domino()) {
                return Some((b, d));
            }
            retry_sleep(attempt);
        }
        None
    }

    /// 多次探测"某格式**始终**不存在"：任一次观测到即判存在
    ///
    /// 用于负向断言——单次读取受 `OpenClipboard` 偶发失败影响会误判为"不存在"，
    /// 故必须在可读的多个时刻都没看到该格式才算通过。
    fn absent_over_probes(mut probe: impl FnMut() -> Option<Vec<u8>>) -> bool {
        for attempt in 0..RETRY_ATTEMPTS {
            if probe().is_some() {
                return false;
            }
            retry_sleep(attempt);
        }
        true
    }

    /// 复制闸门：关闭时 `domino_payload` 返回 `None`（不进入编码）；开启时与直接编码逐字节一致
    ///
    /// 纯计算，不触碰系统剪贴板，故无需 `CLIPBOARD_LOCK`。
    #[test]
    fn test_domino_payload_follows_switch() {
        let mut editor = seeded_editor();

        // 关闭（默认）：闸门短路，不产出 Domino 载荷
        assert!(!editor.domino_clipboard_enabled());
        assert!(
            editor.domino_payload().is_none(),
            "关闭时不得构建 Domino 载荷"
        );
        // 对照组：编码器自身可用，证明上面的 None 来自闸门而非编码失败
        assert!(
            editor.build_clipboard_domino().is_some(),
            "选中非空时编码器应能产出载荷（对照）"
        );

        // 开启：闸门等价于直接编码
        editor.set_domino_clipboard_enabled(true);
        let via_gate = editor.domino_payload().expect("开启时应返回 Domino 载荷");
        let direct = editor
            .build_clipboard_domino()
            .expect("开启时直接编码应成功");
        assert_eq!(via_gate, direct, "开启时闸门应等价于直接编码");
    }

    /// 端到端（真实系统剪贴板）：关闭只写 Lumino 二进制；开启写双格式
    #[test]
    fn test_copy_writes_domino_format_only_when_enabled() {
        let _guard = clipboard_guard();
        let mut editor = seeded_editor();

        // 关闭（默认）：先重试到 Lumino 二进制确实写入，再断言 Domino 格式不存在
        assert!(!editor.domino_clipboard_enabled());
        let lumino = copy_until(&mut editor, sys::get_clipboard_binary)
            .expect("关闭时复制应写入 Lumino 二进制格式（系统剪贴板不可用？）");
        assert!(!lumino.is_empty(), "Lumino 二进制载荷不应为空");
        assert!(
            absent_over_probes(sys::get_clipboard_domino),
            "关闭时不得写入 Domino(MidiPortalSequence) 格式"
        );

        // 开启：同一次剪贴板会话内双格式必须**同时**可见（主格式未被 EmptyClipboard 清掉）
        editor.set_domino_clipboard_enabled(true);
        let (lumino_after, domino) = copy_until_both(&mut editor)
            .expect("开启时应在同一次会话内写入 Lumino 二进制 + Domino(MidiPortalSequence) 双格式");
        assert!(
            domino.starts_with(b"PortalSequenceData"),
            "Domino 载荷应以 PortalSequenceData 固定头开头"
        );
        assert_eq!(
            lumino_after.len(),
            lumino.len(),
            "双格式写入后 Lumino 载荷长度应与关闭时一致（未被覆盖）"
        );
    }

    /// 粘贴闸门：关闭时不消费 Domino 格式，开启时才解析落当前轨
    #[test]
    fn test_paste_ignores_domino_format_when_disabled() {
        let _guard = clipboard_guard();
        let mut editor = seeded_editor();
        let track_notes_before = editor.editor_state.data.track_notes(0).len();
        assert_eq!(track_notes_before, 8, "前置条件：当前轨应有 8 个种子音符");

        // 开启态复制 → 拿到合法 Domino 载荷
        editor.set_domino_clipboard_enabled(true);
        let domino = copy_until(&mut editor, sys::get_clipboard_domino)
            .expect("前提：开启态复制应产出 Domino 载荷");

        // 构造「Lumino 主格式存在但不可解析 + Domino 格式合法」，
        // 使"是否消费 Domino"完全由开关决定；重试直到该状态确实建立
        const GARBAGE: &[u8] = b"not-a-lumino-payload";
        let mut prepared = false;
        for attempt in 0..RETRY_ATTEMPTS {
            let _ = sys::set_clipboard_binary_pair(GARBAGE, &domino);
            if sys::get_clipboard_binary().as_deref() == Some(GARBAGE)
                && sys::get_clipboard_domino().is_some()
            {
                prepared = true;
                break;
            }
            retry_sleep(attempt);
        }
        assert!(
            prepared,
            "前置条件：应能建立「Lumino 主格式不可解析 + Domino 合法」的剪贴板状态"
        );

        // 关闭：不得从 Domino 格式粘贴出新音符
        editor.set_domino_clipboard_enabled(false);
        editor.paste_notes_from_clipboard();
        assert_eq!(
            editor.editor_state.data.track_notes(0).len(),
            track_notes_before,
            "关闭时不得消费 MidiPortalSequence（不应粘贴出新音符）"
        );
        // 强化：此刻剪贴板仍可读且 Domino 载荷仍在 → 上面的"没粘贴"来自闸门，
        // 而不是"根本读不到剪贴板"的假阴性
        assert!(
            sys::get_clipboard_domino().is_some(),
            "关闭态粘贴后 Domino 载荷应仍在剪贴板上（用于排除读取失败导致的假阴性）"
        );

        // 开启：同一载荷应被解析并按 Domino 语义落到当前轨
        editor.set_domino_clipboard_enabled(true);
        let mut pasted = false;
        for attempt in 0..RETRY_ATTEMPTS {
            editor.paste_notes_from_clipboard();
            if editor.editor_state.data.track_notes(0).len() == track_notes_before + 8 {
                pasted = true;
                break;
            }
            retry_sleep(attempt);
        }
        assert!(
            pasted,
            "开启时应从 Domino 格式粘贴出 8 个音符（实得 {}）",
            editor.editor_state.data.track_notes(0).len()
        );
    }
}
