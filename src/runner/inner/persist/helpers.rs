//! 配置持久化的纯判据辅助函数（从 `persist.rs` 拆出，REF-001 < 400 行）。
//!
//! 全部是纯函数、无状态，被 `persist.rs` 的 `config_diff` / 落盘逻辑与
//! `persist/tests.rs` 直接引用；拆开只为让 `persist.rs` 保持可读长度。

/// 空字符串展示为 `(空)`，避免日志与差异对比出现空白。
pub(super) fn display_or_empty(s: &str) -> &str {
    if s.is_empty() { "(空)" } else { s }
}

/// UI-020：语言字段是否应落盘。
///
/// 环境变量（`LUMINO_LANG`）只影响本次启动：来源为 env 且用户本次未显式
/// 改语言时不落盘（防止排障覆盖被任意一次设置保存物化成固定语言）；
/// 用户真实改过语言（`language_changed`）时照常写入意图。
pub(super) fn should_persist_language(language_from_env: bool, language_changed: bool) -> bool {
    !language_from_env || language_changed
}

/// 解析自动保存间隔输入（分钟）：越界值收敛到 1..=120，非法输入返回
/// `None`（落盘时保留旧值，不阻塞其他设置项的保存）。
pub(super) fn parse_autosave_interval_mins(s: &str) -> Option<u32> {
    s.trim().parse::<u32>().ok().map(|v| v.clamp(1, 120))
}

/// 洋葱皮渲染后端是否应落盘（「加载 MIDI 后禁止切换后端」的第三道闸）。
///
/// 两个条件**任一**为真即不写，保留旧配置：
/// - `backend_locked`：设置面板处于锁定会话态（主窗口注入）；
/// - `midi_loaded`：主窗口会话已加载**真实** MIDI/工程（空白工程不算）。
///
/// 为什么两个都要：设置对话框可能在「无 MIDI」时打开、之后才加载 MIDI，
/// 此时对话框面板里的 `backend_locked` 仍是打开那一刻的 `false`——
/// 只信面板字段会把「加载 MIDI 后本不该生效的后端」写到下次启动。
///
/// 为什么判据是 `midi_loaded` 而不是「有没有文档」：启动/新建/关闭工程都会挂一份
/// 2 轨空白 `MidiDocument`，用「有文档」判会把空白工程也挡在落盘之外——
/// 用户在空白工程里选的后端永远存不下来（见 `host::session_lock`）。
pub(super) fn should_persist_onion_backend(backend_locked: bool, midi_loaded: bool) -> bool {
    !backend_locked && !midi_loaded
}
