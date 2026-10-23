//! 父进程侧的探针监督：启动、超时强杀、收割与解析。
//!
//! 关键不变量：
//! - **超时必须能真正终止执行体**。线程做不到（Rust 无线程取消），进程可以
//!   （`Child::kill` 在 Windows 上是 `TerminateProcess`）；
//! - **不留下残留进程**：被杀后必须 `wait()` 收割，避免僵尸 / 句柄泄漏；
//! - **父进程崩溃也不留孤儿**：Windows 下把子进程绑进 `KILL_ON_JOB_CLOSE` 的 Job，
//!   父进程一死句柄关闭，OS 连带杀子进程——否则重启后的新实例会被孤儿探针继续拖死。

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use lumino_core::trf;
use lumino_gfx::device_check::{FallbackDiagnostics, GpuCheckFailure, GpuCheckReport};

use super::child::Mode;
use super::{CHILD_FLAG, FALLBACK_FLAG, MODE_FLAG, TIMEOUT_FLAG, fallback_arg, wire};

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
pub(crate) fn run(timeout: Duration, when: FallbackDiagnostics) -> GpuCheckReport {
    match child_command(timeout, Mode::Full, when) {
        Ok((cmd, out)) => run_report_command(cmd, out, timeout),
        Err(e) => aborted(e),
    }
}

/// 在子进程中执行一次廉价指纹探测，超时强杀。
///
/// 返回 `None` 表示超时 / 启动失败 / 回传不可解析，调用方按 **cache miss** 处理。
pub(crate) fn run_fingerprints(timeout: Duration) -> Option<Vec<String>> {
    match child_command(timeout, Mode::Fingerprints, FallbackDiagnostics::Never) {
        Ok((cmd, out)) => run_fingerprints_command(cmd, out, timeout),
        Err(e) => {
            tracing::warn!("廉价探测子进程启动失败（按 cache miss 处理）: {e}");
            None
        }
    }
}

/// 构造「重新拉起自身」的命令与结果文件路径。
fn child_command(
    timeout: Duration,
    mode: Mode,
    when: FallbackDiagnostics,
) -> Result<(Command, PathBuf), String> {
    let exe = std::env::current_exe().map_err(|e| trf!("无法定位程序自身路径: {e}", e = e))?;
    let out = probe_output_path();
    let _ = std::fs::remove_file(&out);

    let mut cmd = Command::new(&exe);
    cmd.args(child_args(&out, timeout, mode, when))
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
        mode = mode.as_arg(),
        timeout_ms = timeout.as_millis(),
        "启动 GPU 检测子进程"
    );
    Ok((cmd, out))
}

/// 构造子进程参数序列。
///
/// 与 `child::parse_args_from` 是**成对契约**：参数名 / 顺序 / 取值由单测锁定
/// （`test_parent_args_match_child_parser`），改动任一侧都会红。
fn child_args(
    out: &Path,
    timeout: Duration,
    mode: Mode,
    when: FallbackDiagnostics,
) -> Vec<OsString> {
    vec![
        OsString::from(CHILD_FLAG),
        out.as_os_str().to_owned(),
        OsString::from(MODE_FLAG),
        OsString::from(mode.as_arg()),
        OsString::from(FALLBACK_FLAG),
        OsString::from(fallback_arg(when)),
        OsString::from(TIMEOUT_FLAG),
        OsString::from(timeout.as_millis().to_string()),
    ]
}

/// 一次监督的结果（spawn 成功后）
struct Waited {
    /// 是否因超时被强杀
    killed: bool,
    /// 子进程退出状态（超时强杀时为 `None`）
    status: Option<ExitStatus>,
    /// stderr 尾部（诊断用）
    stderr: String,
    /// 从启动到结束的耗时
    elapsed: Duration,
}

/// 启动并等待子进程；`spawn` 失败返回错误消息。
///
/// Job Object 在本函数作用域内持有：父进程若在本函数返回前崩溃，句柄随进程关闭，
/// OS 连带杀死子进程（Windows）。
fn spawn_and_wait(mut cmd: Command, timeout: Duration) -> Result<Waited, String> {
    let mut child = cmd
        .spawn()
        .map_err(|e| trf!("检测进程启动失败: {e}", e = e))?;

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
    let supervision = supervise(&mut child, Instant::now() + timeout + KILL_GRACE);
    let stderr = stderr
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();

    Ok(Waited {
        killed: supervision.killed,
        status: supervision.status,
        stderr,
        elapsed: supervision.elapsed,
    })
}

/// 执行并解析完整检测结果（`Command` 由调用方构造，便于单测注入假子进程）。
fn run_report_command(cmd: Command, out: PathBuf, timeout: Duration) -> GpuCheckReport {
    let report = match spawn_and_wait(cmd, timeout) {
        Err(e) => {
            let _ = std::fs::remove_file(&out);
            return aborted(e);
        }
        Ok(waited) if waited.killed => {
            let mut report = GpuCheckReport::failed(GpuCheckFailure::Timeout);
            report.duration_ms = waited.elapsed.as_millis() as u64;
            tracing::warn!(
                elapsed_ms = report.duration_ms,
                "GPU 检测子进程超时，已强制终止"
            );
            report
        }
        Ok(waited) => match wire::read_report(&out) {
            Ok(report) => report,
            Err(e) => {
                let mut msg = trf!("检测进程未回传有效结果: {e}", e = e);
                if let Some(status) = waited.status {
                    msg.push_str(&trf!("（退出状态 {status}）", status = status));
                }
                if !waited.stderr.is_empty() {
                    msg.push_str("\n--- probe stderr ---\n");
                    msg.push_str(&waited.stderr);
                }
                tracing::error!("GPU 检测子进程失败: {e}");
                aborted(msg)
            }
        },
    };

    let _ = std::fs::remove_file(&out);
    report
}

/// 执行并解析廉价探测结果；任何失败都折算为 `None`（cache miss）。
fn run_fingerprints_command(cmd: Command, out: PathBuf, timeout: Duration) -> Option<Vec<String>> {
    let outcome = match spawn_and_wait(cmd, timeout) {
        Err(e) => {
            tracing::warn!("廉价探测子进程启动失败（按 cache miss 处理）: {e}");
            None
        }
        Ok(waited) if waited.killed => {
            tracing::warn!("廉价探测子进程超时，已强制终止（按 cache miss 处理）");
            None
        }
        Ok(_) => match wire::read_fingerprints(&out) {
            Ok(fingerprints) => Some(fingerprints),
            Err(e) => {
                tracing::warn!("廉价探测子进程未回传有效指纹（按 cache miss 处理）: {e}");
                None
            }
        },
    };

    let _ = std::fs::remove_file(&out);
    outcome
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
mod tests;
