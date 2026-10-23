//! 父进程侧的探针监督：启动、超时强杀、收割与解析。
//!
//! 关键不变量：
//! - **超时必须能真正终止执行体**。线程做不到（Rust 无线程取消），进程可以
//!   （`Child::kill` 在 Windows 上是 `TerminateProcess`）；
//! - **不留下残留进程**：被杀后必须 `wait()` 收割，避免僵尸 / 句柄泄漏；
//! - **父进程崩溃也不留孤儿**：Windows 下把子进程绑进 `KILL_ON_JOB_CLOSE` 的 Job，
//!   父进程一死句柄关闭，OS 连带杀子进程——否则重启后的新实例会被孤儿探针继续拖死。

#![cfg_attr(not(windows), allow(dead_code))]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use lumino_core::trf;
use lumino_gfx::device_check::{FallbackDiagnostics, GpuCheckFailure, GpuCheckReport};

use super::{CHILD_FLAG, FALLBACK_FLAG, TIMEOUT_FLAG, fallback_arg, wire};

#[cfg(windows)]
mod job;
mod stderr;

/// 父进程硬杀相对「语义超时」的宽限。
///
/// 子进程自带看门狗（到点先写结构化超时结果），本宽限只兜「看门狗线程也被驱动
/// 卡住」的极端情况——此时 `TerminateProcess` 是唯一能真正解除锁占用的手段。
pub(crate) const KILL_GRACE: Duration = Duration::from_millis(500);

/// 等待子进程的轮询间隔：10ms × 最多约 350 次，开销可忽略，换取零额外线程/信号。
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// 每次探针调用的唯一序号（结果文件去重，避免两次检测互相覆盖）
static INVOCATION_SEQ: AtomicU64 = AtomicU64::new(0);

/// 在子进程中执行一次完整检测，超时强杀。
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn run(timeout: Duration, when: FallbackDiagnostics) -> GpuCheckReport {
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => return aborted(trf!("无法定位程序自身路径: {e}", e = e)),
    };
    let out = probe_output_path();
    let _ = std::fs::remove_file(&out);

    let mut cmd = Command::new(&exe);
    cmd.args(child_args(&out, timeout, when))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // debug 构建是 console 子系统，直接派生会闪黑框；release 是 windows 子系统，
        // 该标志同样无害。
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    tracing::info!(
        exe = %exe.display(),
        timeout_ms = timeout.as_millis(),
        "启动 GPU 检测子进程"
    );
    run_command(cmd, out, timeout)
}

/// 构造子进程参数序列。
///
/// 与 `child::parse_args_from` 是**成对契约**：参数名 / 顺序 / 取值由单测锁定
/// （`test_parent_args_match_child_parser`），改动任一侧都会红。
fn child_args(out: &Path, timeout: Duration, when: FallbackDiagnostics) -> Vec<OsString> {
    vec![
        OsString::from(CHILD_FLAG),
        out.as_os_str().to_owned(),
        OsString::from(FALLBACK_FLAG),
        OsString::from(fallback_arg(when)),
        OsString::from(TIMEOUT_FLAG),
        OsString::from(timeout.as_millis().to_string()),
    ]
}

/// 执行并监督一个探针子进程（`Command` 由调用方构造，便于单测注入假子进程）。
fn run_command(mut cmd: Command, out: PathBuf, timeout: Duration) -> GpuCheckReport {
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            let _ = std::fs::remove_file(&out);
            return aborted(trf!("检测进程启动失败: {e}", e = e));
        }
    };

    // Windows：父进程崩溃时连带杀死探针，避免孤儿继续持有驱动锁
    #[cfg(windows)]
    let _job = match job::KillOnCloseJob::assign(&child) {
        Some(job) => Some(job),
        None => {
            tracing::warn!(
                "探针子进程未能绑定 Job Object：父进程崩溃时可能留下孤儿（超时强杀不受影响）"
            );
            None
        }
    };

    let stderr = child.stderr.take().and_then(stderr::spawn_reader);
    let deadline = Instant::now() + timeout + KILL_GRACE;
    let supervision = supervise(&mut child, deadline);
    let tail = stderr
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();

    let report = if supervision.killed {
        let mut report = GpuCheckReport::failed(GpuCheckFailure::Timeout);
        report.duration_ms = supervision.elapsed.as_millis() as u64;
        tracing::warn!(
            elapsed_ms = report.duration_ms,
            "GPU 检测子进程超时，已强制终止"
        );
        report
    } else {
        match wire::read_report(&out) {
            Ok(report) => report,
            Err(e) => {
                let mut msg = trf!("检测进程未回传有效结果: {e}", e = e);
                if let Some(status) = supervision.status {
                    msg.push_str(&trf!("（退出状态 {status}）", status = status));
                }
                if !tail.is_empty() {
                    msg.push_str("\n--- probe stderr ---\n");
                    msg.push_str(&tail);
                }
                tracing::error!("GPU 检测子进程失败: {e}");
                aborted(msg)
            }
        }
    };

    let _ = std::fs::remove_file(&out);
    report
}

/// 一次监督的结果
struct Supervision {
    /// 是否因超时被强杀
    killed: bool,
    /// 子进程退出状态（超时强杀时为 `None`）
    status: Option<std::process::ExitStatus>,
    /// 从启动到结束的耗时
    elapsed: Duration,
}

/// 等待子进程结束；到点则强杀并收割。
///
/// 无论走哪条路径都执行 `wait()`：Windows 上不收割会留下僵尸句柄，任务管理器里
/// 仍能看到进程条目。
fn supervise(child: &mut Child, deadline: Instant) -> Supervision {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Supervision {
                    killed: false,
                    status: Some(status),
                    elapsed: started.elapsed(),
                };
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!("等待 GPU 检测子进程失败: {e}");
                let _ = child.kill();
                let _ = child.wait();
                return Supervision {
                    killed: false,
                    status: None,
                    elapsed: started.elapsed(),
                };
            }
        }

        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Supervision {
                killed: true,
                status: None,
                elapsed: started.elapsed(),
            };
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// 本次调用的唯一结果文件路径
fn probe_output_path() -> PathBuf {
    let seq = INVOCATION_SEQ.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "lumino-device-probe-{}-{seq}.json",
        std::process::id()
    ))
}

/// 构造「执行环境异常」报告
fn aborted(reason: String) -> GpuCheckReport {
    GpuCheckReport::failed(GpuCheckFailure::Aborted(reason))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 立即退出的假子进程
    fn immediate_exit_command() -> Command {
        #[cfg(windows)]
        {
            let mut cmd = Command::new("cmd");
            cmd.args(["/C", "exit", "0"]);
            cmd
        }
        #[cfg(not(windows))]
        {
            let mut cmd = Command::new("true");
            cmd
        }
    }

    /// 会睡很久的假子进程（用于验证强杀）
    fn sleeping_command() -> Command {
        #[cfg(windows)]
        {
            let mut cmd = Command::new("cmd");
            cmd.args(["/C", "ping", "-n", "30", "127.0.0.1"]);
            cmd
        }
        #[cfg(not(windows))]
        {
            let mut cmd = Command::new("sleep");
            cmd.arg("30");
            cmd
        }
    }

    fn unique_out(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "lumino-probe-supervisor-{tag}-{}-{nanos}.json",
            std::process::id()
        ))
    }

    /// 挂起的子进程必须在宽限后被杀且被收割（无僵尸、无残留）
    #[test]
    fn test_hung_child_is_killed_and_reaped() {
        let mut cmd = sleeping_command();
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
        let mut child = cmd.spawn().expect("假子进程应能启动");
        let started = Instant::now();

        let supervision = supervise(&mut child, Instant::now() + Duration::from_millis(200));

        assert!(supervision.killed, "超过 deadline 必须判为强杀");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "强杀不应额外等待（实际 {:?}）",
            started.elapsed()
        );
        assert!(
            child.try_wait().expect("收割状态可读").is_some(),
            "被强杀的子进程必须已 wait() 收割，否则残留僵尸/句柄"
        );
    }

    /// 子进程正常退出但没写结果 → 执行环境异常（不得误判为驱动失败）
    #[test]
    fn test_child_exits_without_report_is_aborted() {
        let out = unique_out("missing");
        let _ = std::fs::remove_file(&out);
        let mut cmd = immediate_exit_command();
        cmd.stdout(Stdio::null()).stderr(Stdio::null());

        let report = run_command(cmd, out, Duration::from_secs(2));

        assert!(
            matches!(report.failure, Some(GpuCheckFailure::Aborted(_))),
            "无结果的退出应归为 Aborted，实际 {:?}",
            report.failure
        );
        assert!(!report.passed);
    }

    /// 子进程写出的结果必须被完整读回（预置文件 + 立即退出，模拟子进程已回传）
    #[test]
    fn test_child_report_is_read_back() {
        let out = unique_out("ok");
        let expected = GpuCheckReport {
            passed: true,
            failure: None,
            required_backend: lumino_gfx::device_check::required_backend_name(),
            adapters: Vec::new(),
            fallback_adapters: Vec::new(),
            fingerprint: Some("Vulkan|gpu|drv|info|DiscreteGpu".into()),
            duration_ms: 123,
        };
        wire::write_report(&out, &expected).expect("预置结果失败");

        let mut cmd = immediate_exit_command();
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
        let report = run_command(cmd, out, Duration::from_secs(2));

        assert!(
            report.passed,
            "通过结果应原样回传，实际 {:?}",
            report.failure
        );
        assert_eq!(report.fingerprint, expected.fingerprint);
        assert_eq!(report.duration_ms, expected.duration_ms);
    }

    /// 子进程写出损坏结果 → 执行环境异常，并带上退出状态
    #[test]
    fn test_corrupted_report_is_aborted_with_status() {
        let out = unique_out("garbage");
        std::fs::write(&out, b"}{").expect("写坏数据失败");

        let mut cmd = immediate_exit_command();
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
        let report = run_command(cmd, out, Duration::from_secs(2));

        match report.failure {
            Some(GpuCheckFailure::Aborted(message)) => {
                assert!(message.contains("退出状态"), "应附带退出状态: {message}");
            }
            other => panic!("期望 Aborted，实际 {other:?}"),
        }
    }

    /// 结果文件路径必须是「每次调用唯一」，避免两次检测互相覆盖
    #[test]
    fn test_probe_output_paths_are_unique() {
        let first = probe_output_path();
        let second = probe_output_path();
        assert_ne!(first, second);
    }

    /// 父进程构造的参数必须能被子进程解析（两侧契约防漂移）
    #[test]
    fn test_parent_args_match_child_parser() {
        let out = PathBuf::from("probe-out.json");
        let args = child_args(
            &out,
            Duration::from_millis(3000),
            FallbackDiagnostics::OnFailure,
        );
        let parsed =
            super::super::child::parse_args_from(args).expect("子进程必须能解析父进程构造的参数");

        assert_eq!(parsed.out, out);
        assert_eq!(parsed.when, FallbackDiagnostics::OnFailure);
        assert_eq!(parsed.timeout, Duration::from_millis(3000));
    }

    /// 未知 `--` 参数必须报错（避免拼写错误静默跑成默认策略）
    #[test]
    fn test_child_parser_rejects_unknown_flag() {
        let args = vec![OsString::from("--device-check-prob"), OsString::from("x")];
        assert!(super::super::child::parse_args_from(args).is_err());
    }
}
