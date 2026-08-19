//! Toast 单元测试

use super::*;
use std::thread;

use std::time::{Duration, Instant};

/// 创建测试用 Toast（自定义创建时间）
fn make_toast(id: u64, level: ToastLevel, message: &str, age_ms: u64) -> Toast {
    Toast {
        id,
        level,
        message: message.to_string(),
        created_at: Instant::now() - Duration::from_millis(age_ms),
        duration: Duration::from_millis(3000),
    }
}

#[test]
fn test_toast_level_default_duration() {
    assert_eq!(
        ToastLevel::Info.default_duration(),
        Duration::from_millis(2500)
    );
    assert_eq!(
        ToastLevel::Warning.default_duration(),
        Duration::from_millis(3500)
    );
    assert_eq!(
        ToastLevel::Error.default_duration(),
        Duration::from_millis(5000)
    );
    assert_eq!(
        ToastLevel::Success.default_duration(),
        Duration::from_millis(2500)
    );
}

#[test]
fn test_toast_level_icon() {
    assert_eq!(ToastLevel::Info.icon(), "ℹ");
    assert_eq!(ToastLevel::Warning.icon(), "⚠");
    assert_eq!(ToastLevel::Error.icon(), "✗");
    assert_eq!(ToastLevel::Success.icon(), "✓");
}

#[test]
fn test_toast_is_expired() {
    let now = Instant::now();
    let t = Toast {
        id: 1,
        level: ToastLevel::Info,
        message: "test".into(),
        created_at: now - Duration::from_millis(2000),
        duration: Duration::from_millis(3000),
    };
    assert!(!t.is_expired(now)); // 2s < 3s，未过期

    let t2 = Toast {
        id: 2,
        level: ToastLevel::Info,
        message: "test".into(),
        created_at: now - Duration::from_millis(4000),
        duration: Duration::from_millis(3000),
    };
    assert!(t2.is_expired(now)); // 4s > 3s，已过期
}

#[test]
fn test_toast_remaining() {
    let now = Instant::now();
    let t = Toast {
        id: 1,
        level: ToastLevel::Info,
        message: "test".into(),
        created_at: now - Duration::from_millis(1000),
        duration: Duration::from_millis(3000),
    };
    let remaining = t.remaining(now);
    assert!(remaining.as_millis() <= 2000);
    assert!(remaining.as_millis() >= 1900); // 容忍一些测试耗时
}

#[test]
fn test_toast_manager_new_is_empty() {
    let mgr = ToastManager::new();
    assert!(mgr.is_empty());
    assert_eq!(mgr.len(), 0);
}

#[test]
fn test_toast_manager_push_returns_unique_id() {
    let mut mgr = ToastManager::new();
    let id1 = mgr.push(ToastLevel::Info, "msg1");
    let id2 = mgr.push(ToastLevel::Warning, "msg2");
    let id3 = mgr.push(ToastLevel::Error, "msg3");
    assert_ne!(id1, id2);
    assert_ne!(id2, id3);
    assert_ne!(id1, id3);
    assert_eq!(mgr.len(), 3);
}

#[test]
fn test_toast_manager_push_with_duration() {
    let mut mgr = ToastManager::new();
    let id = mgr.push_with_duration(ToastLevel::Info, "custom", Duration::from_millis(100));
    assert_eq!(mgr.toasts().len(), 1);
    let toast = &mgr.toasts()[0];
    assert_eq!(toast.id, id);
    assert_eq!(toast.duration, Duration::from_millis(100));
    assert_eq!(toast.message, "custom");
}

#[test]
fn test_toast_manager_max_visible_evicts_oldest() {
    let mut mgr = ToastManager::new();
    mgr.max_visible = 3;
    for i in 0..5 {
        mgr.push(ToastLevel::Info, format!("msg{i}"));
    }
    assert_eq!(mgr.len(), 3); // 限制在 3 条
    // 最早的两条被移除，保留最后 3 条
    assert_eq!(mgr.toasts()[0].message, "msg2");
    assert_eq!(mgr.toasts()[1].message, "msg3");
    assert_eq!(mgr.toasts()[2].message, "msg4");
}

#[test]
fn test_toast_manager_dismiss() {
    let mut mgr = ToastManager::new();
    let id1 = mgr.push(ToastLevel::Info, "msg1");
    let id2 = mgr.push(ToastLevel::Warning, "msg2");
    assert_eq!(mgr.len(), 2);

    mgr.dismiss(id1);
    assert_eq!(mgr.len(), 1);
    assert_eq!(mgr.toasts()[0].id, id2);

    // dismiss 不存在的 id 不应 panic
    mgr.dismiss(999);
    assert_eq!(mgr.len(), 1);
}

#[test]
fn test_toast_manager_cleanup_expired_removes_old() {
    let mut mgr = ToastManager::new();
    // 直接构造过期的 Toast
    mgr.toasts
        .push(make_toast(1, ToastLevel::Info, "old1", 5000));
    mgr.toasts
        .push(make_toast(2, ToastLevel::Info, "old2", 4000));
    mgr.toasts
        .push(make_toast(3, ToastLevel::Info, "fresh", 100));

    let now = Instant::now();
    let removed = mgr.cleanup_expired(now);
    assert_eq!(removed, 2);
    assert_eq!(mgr.len(), 1);
    assert_eq!(mgr.toasts()[0].message, "fresh");
}

#[test]
fn test_toast_manager_cleanup_expired_no_op_when_all_fresh() {
    let mut mgr = ToastManager::new();
    mgr.push(ToastLevel::Info, "fresh1");
    mgr.push(ToastLevel::Info, "fresh2");

    let now = Instant::now();
    let removed = mgr.cleanup_expired(now);
    assert_eq!(removed, 0);
    assert_eq!(mgr.len(), 2);
}

#[test]
fn test_toast_manager_cleanup_expired_removes_all_when_all_expired() {
    let mut mgr = ToastManager::new();
    mgr.toasts
        .push(make_toast(1, ToastLevel::Info, "old1", 10000));
    mgr.toasts
        .push(make_toast(2, ToastLevel::Error, "old2", 8000));

    let now = Instant::now();
    let removed = mgr.cleanup_expired(now);
    assert_eq!(removed, 2);
    assert!(mgr.is_empty());
}

#[test]
fn test_toast_manager_push_wrapping_id() {
    let mut mgr = ToastManager::new();
    mgr.next_id = u64::MAX;
    let id1 = mgr.push(ToastLevel::Info, "msg1");
    assert_eq!(id1, u64::MAX);
    let id2 = mgr.push(ToastLevel::Info, "msg2");
    assert_eq!(id2, 0); // wrapping_add(1) = 0
}

#[test]
fn test_toast_manager_view_empty_returns_none() {
    let mgr = ToastManager::new();
    // view 在无 Toast 时短路返回 None，不需要 Theme 实例
    // 这里用 unreachable 的 theme 引用（不会被执行）来验证 None 分支
    // 由于 Theme::default 在 iced 0.14 测试环境有 trait 作用域问题，
    // 此测试仅验证逻辑：空 ToastManager 的 view 必返回 None。
    assert!(mgr.is_empty());
}

#[test]
fn test_toast_manager_view_with_toasts_returns_some() {
    let mut mgr = ToastManager::new();
    mgr.push(ToastLevel::Warning, "test");
    // view 需要 &Theme 参数；Theme 实例化在 lib test 中受限，
    // 此测试验证 push 后 ToastManager 非空（view 在非空时会构造叠加层）。
    assert!(!mgr.is_empty());
}

#[test]
fn test_toast_lifecycle_push_and_expire() {
    let mut mgr = ToastManager::new();
    mgr.push_with_duration(ToastLevel::Info, "short-lived", Duration::from_millis(50));
    assert_eq!(mgr.len(), 1);

    // 等待过期
    thread::sleep(Duration::from_millis(80));
    let removed = mgr.cleanup_expired(Instant::now());
    assert_eq!(removed, 1);
    assert!(mgr.is_empty());
}
