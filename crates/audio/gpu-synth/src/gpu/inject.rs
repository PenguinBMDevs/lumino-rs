//! R2/R5 测试注入：LGS **独立 device** 的 safe-point destroy
//! （仅 `gpu-reconnect-test` 特性构建；默认构建不编译本模块）。
//!
//! 模式与编辑器侧（gfx `device_status`）一致：注入线程只置位请求，
//! 由**渲染块安全点**（`GpuSynth::render_block` 首行，无在途 GPU 调用）
//! 执行 destroy 并单次 poll 同步送达丢失回调（R1 不变量）。

use std::sync::atomic::{AtomicBool, Ordering};

/// 待执行注入请求（注入线程置位；渲染块安全点消费）。
static INJECTION_PENDING: AtomicBool = AtomicBool::new(false);

/// 消费一次待执行注入请求（渲染块安全点调用）。
pub(crate) fn take_injection_pending() -> bool {
    INJECTION_PENDING.swap(false, Ordering::SeqCst)
}

/// 依 env 武装注入线程（首次 `create_gpu_context` 时调用，幂等）。
///
/// env：
/// - `LUMINO_TEST_DEVICE_LOST_LGS`：非 "0" 启用；
/// - `LUMINO_TEST_DEVICE_LOST_LGS_DELAY`：首次延时秒（默认 5）；
/// - `LUMINO_TEST_DEVICE_LOST_LGS_INTERVAL`：重复间隔秒（0/缺省 = 单次）。
pub(crate) fn arm_injection_if_requested() {
    static SPAWNED: AtomicBool = AtomicBool::new(false);
    if SPAWNED.swap(true, Ordering::SeqCst) {
        return;
    }
    let Some(value) = std::env::var_os("LUMINO_TEST_DEVICE_LOST_LGS") else {
        return;
    };
    if value.to_string_lossy() == "0" {
        return;
    }
    let delay = env_secs("LUMINO_TEST_DEVICE_LOST_LGS_DELAY", 5);
    let interval = env_secs("LUMINO_TEST_DEVICE_LOST_LGS_INTERVAL", 0);
    tracing::warn!(
        "[gpu-reconnect-test] LGS 注入已武装：{delay}s 后请求 destroy（interval={interval}s）"
    );
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(delay));
        loop {
            INJECTION_PENDING.store(true, Ordering::SeqCst);
            tracing::warn!(
                "[gpu-reconnect-test] LGS 注入已请求：等待渲染块安全点执行 device.destroy()"
            );
            if interval == 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(interval));
        }
    });
}

#[cfg(feature = "gpu-reconnect-test")]
fn env_secs(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
