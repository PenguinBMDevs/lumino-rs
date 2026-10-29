//! 配置落盘守卫的真值表测试
//!
//! 抽出独立文件的原因：`persist.rs` 已接近 400 行上限（AGENTS.md 文件长度约束），
//! 把「纯函数守卫」的用例放这里，主文件只保留实现与调用点。

use super::{should_persist_language, should_persist_onion_backend};

/// UI-020：语言落盘守卫真值表——env 来源且未改语言时不落盘，其余照常。
#[test]
fn test_should_persist_language_guard() {
    assert!(
        should_persist_language(false, false),
        "非 env 来源：照常落盘"
    );
    assert!(
        should_persist_language(false, true),
        "非 env 来源：照常落盘"
    );
    assert!(
        !should_persist_language(true, false),
        "env 来源且未改语言：不得物化（老配置场景的核心守卫）"
    );
    assert!(
        should_persist_language(true, true),
        "env 来源但用户显式改语言：照常落盘意图"
    );
}

/// 洋葱皮后端落盘守卫真值表。
///
/// 三层闸的第三层：只要「面板锁定」或「主窗口已加载真实 MIDI」任一成立就不写。
///
/// 判据是 `midi_loaded`（会话锁）而**不是**「有没有文档」：启动/新建/关闭工程都会挂
/// 一份 2 轨空白 `MidiDocument`，若按「有文档」判，空白工程里选的后端永远存不下来。
#[test]
fn test_should_persist_onion_backend_guard() {
    assert!(
        should_persist_onion_backend(false, false),
        "未锁定 + 空白工程（未加载 MIDI）：必须落盘——用户的空白工程选择要能保存"
    );
    assert!(
        !should_persist_onion_backend(true, false),
        "面板已锁定：不落盘"
    );
    assert!(
        !should_persist_onion_backend(false, true),
        "字段过期（设置对话框先开、MIDI 后载）：主窗口已加载真实 MIDI 即不落盘——兜底的核心"
    );
    assert!(
        !should_persist_onion_backend(true, true),
        "双重锁定：不落盘"
    );
}
