//! panic 安全守卫：保证 move/借出的 API 不因 worker 或同步调用 panic 失联。
//!
//! AUD-008 同域加固：`thread::spawn + move MidiApi` 的唯一归还路径是 channel，
//! panic 会跳过 `send`；`run_guarded` 用 `catch_unwind` 截断，把值与 panic
//! 语义一起返回。`guard_mut` 用于同步路径——API 仍由 `MidiManager` 持有，
//! panic 后调用方可继续按失败/重初始化处理。

use std::panic::{AssertUnwindSafe, catch_unwind};

/// 守卫执行结果：正常返回值，或 panic 的可读消息。
#[derive(Debug)]
pub(super) enum Guarded<T> {
    /// 正常返回（含业务 Err）
    Done(T),
    /// 执行中 panic，已被截断
    Panicked(String),
}

/// 提取 panic payload 的可读消息（与 `logging.rs` 的 panic hook 同口径）。
pub(super) fn panic_payload_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "未知 panic payload".to_string()
    }
}

/// 对 move 进来的值执行 `f`；无论是否 panic，值都原样返回。
///
/// `AssertUnwindSafe` 的成立依据：panic 后不假设值内部状态仍可正常使用，
/// 调用方必须按失败处理（重试/降级/重建），而不是继续消费其运行时状态。
pub(super) fn run_guarded<T, R>(mut value: T, f: impl FnOnce(&mut T) -> R) -> (T, Guarded<R>) {
    let caught = catch_unwind(AssertUnwindSafe(|| f(&mut value)));
    let outcome = match caught {
        Ok(result) => Guarded::Done(result),
        Err(payload) => {
            let message = panic_payload_message(&*payload);
            // payload 的 Drop 不参与二次展开（Drop 内再 panic 会 abort 进程）。
            std::mem::forget(payload);
            Guarded::Panicked(message)
        }
    };
    (value, outcome)
}

/// 对借出的值执行 `f`；panic 被截断，调用方仍持有原值。
///
/// 与 [`run_guarded`] 相同的状态假设：panic 后按失败处理，不信任内部运行时状态。
pub(super) fn guard_mut<T, R>(value: &mut T, f: impl FnOnce(&mut T) -> R) -> Guarded<R> {
    match catch_unwind(AssertUnwindSafe(|| f(value))) {
        Ok(result) => Guarded::Done(result),
        Err(payload) => {
            let message = panic_payload_message(&*payload);
            std::mem::forget(payload);
            Guarded::Panicked(message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_guarded_returns_value_after_panic() {
        let (value, outcome) = run_guarded(vec![1, 2], |v| {
            v.push(3);
            panic!("boom");
        });
        assert_eq!(value, vec![1, 2, 3], "panic 后值必须原样返回");
        assert!(matches!(outcome, Guarded::Panicked(msg) if msg.contains("boom")));
    }

    #[test]
    fn run_guarded_preserves_business_error() {
        let (value, outcome) = run_guarded(7u8, |_| Err::<(), String>("err".to_string()));
        assert_eq!(value, 7);
        assert!(matches!(outcome, Guarded::Done(Err(e)) if e == "err"));
    }

    #[test]
    fn guard_mut_keeps_value_after_panic() {
        let mut value = vec![1];
        let outcome = guard_mut(&mut value, |v| {
            v.push(2);
            panic!("boom");
        });
        assert_eq!(value, vec![1, 2], "panic 后借用值仍可访问");
        assert!(matches!(outcome, Guarded::Panicked(_)));
    }

    #[test]
    fn panic_message_extracts_str_string_and_unknown() {
        let text: &str = "str payload";
        let payload: &(dyn std::any::Any + Send) = &text;
        assert_eq!(panic_payload_message(payload), "str payload");

        let owned = "owned payload".to_string();
        let payload: &(dyn std::any::Any + Send) = &owned;
        assert_eq!(panic_payload_message(payload), "owned payload");

        let unknown = 42u32;
        let payload: &(dyn std::any::Any + Send) = &unknown;
        assert_eq!(panic_payload_message(payload), "未知 panic payload");
    }
}
