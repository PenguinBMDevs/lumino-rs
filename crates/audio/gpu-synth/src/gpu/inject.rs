//! R2/R5 测试注入：LGS **独立 device** 的 safe-point destroy
//! （仅 `gpu-reconnect-test` 特性构建；默认构建不编译本模块）。
//!
//! 模式与编辑器侧（gfx `device_status`）一致：注入线程只置位请求，
//! 由**渲染块安全点**（`GpuSynth::render_block` 首行，无在途 GPU 调用）
//! 执行 destroy 并单次 poll 同步送达丢失回调（R1 不变量）。

use std::sync::atomic::{AtomicBool, Ordering};

/// 待执行注入请求（注入线程置位；渲染块安全点消费）。
static INJECTION_PENDING: AtomicBool = AtomicBool::new(false);

/// 离线导出专用待执行注入请求（REND-019 #152 归因验证用）。
///
/// 仅被 **offline 上下文**的渲染块消费——现有注入在实时引擎持续渲染时会被
/// 实时块瞬间截胡，无法定向命中导出实例；此队列保证"导出设备的单独丢失"
/// 可被确定性复现（同时验证：不影响实时 LGS、有界读回快速收场）。
static INJECTION_PENDING_OFFLINE: AtomicBool = AtomicBool::new(false);

/// 消费一次待执行注入请求（渲染块安全点调用）。
///
/// `is_offline` = 当前渲染块的上下文类型：实时与离线各消费各自的队列，
/// 互不截胡（定向验证/诊断可预期）。
pub(crate) fn take_injection_pending(is_offline: bool) -> bool {
    if is_offline {
        INJECTION_PENDING_OFFLINE.swap(false, Ordering::SeqCst)
    } else {
        INJECTION_PENDING.swap(false, Ordering::SeqCst)
    }
}

/// 依 env 武装注入线程（首次 `create_gpu_context` 时调用，幂等）。
///
/// env（实时 LGS）：
/// - `LUMINO_TEST_DEVICE_LOST_LGS`：非 "0" 启用；
/// - `LUMINO_TEST_DEVICE_LOST_LGS_DELAY`：首次延时秒（默认 5）；
/// - `LUMINO_TEST_DEVICE_LOST_LGS_INTERVAL`：重复间隔秒（0/缺省 = 单次）。
///
/// env（离线导出定向，REND-019 #152）：同名 `_LGS_OFFLINE[_DELAY/_INTERVAL]`
/// 前缀，仅命中离线导出实例的 device。
pub(crate) fn arm_injection_if_requested() {
    static SPAWNED: AtomicBool = AtomicBool::new(false);
    if SPAWNED.swap(true, Ordering::SeqCst) {
        return;
    }
    arm_one(
        "LUMINO_TEST_DEVICE_LOST_LGS",
        "LUMINO_TEST_DEVICE_LOST_LGS_DELAY",
        "LUMINO_TEST_DEVICE_LOST_LGS_INTERVAL",
        false,
    );
    arm_one(
        "LUMINO_TEST_DEVICE_LOST_LGS_OFFLINE",
        "LUMINO_TEST_DEVICE_LOST_LGS_OFFLINE_DELAY",
        "LUMINO_TEST_DEVICE_LOST_LGS_OFFLINE_INTERVAL",
        true,
    );
}

fn arm_one(env: &str, delay_env: &str, interval_env: &str, offline: bool) {
    let Some(value) = std::env::var_os(env) else {
        return;
    };
    if value.to_string_lossy() == "0" {
        return;
    }
    let delay = env_secs(delay_env, 5);
    let interval = env_secs(interval_env, 0);
    tracing::warn!(
        "[gpu-reconnect-test] {} 注入已武装：{delay}s 后请求 destroy（interval={interval}s）",
        if offline { "LGS 离线导出" } else { "LGS" }
    );
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(delay));
        loop {
            if offline {
                INJECTION_PENDING_OFFLINE.store(true, Ordering::SeqCst);
            } else {
                INJECTION_PENDING.store(true, Ordering::SeqCst);
            }
            tracing::warn!(
                "[gpu-reconnect-test] LGS 注入已请求：等待渲染块安全点执行 device.destroy()（offline={offline}）"
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
