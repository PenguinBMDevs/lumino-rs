//! 子进程侧：执行检测、看门狗兜底、原子回传结果。
//!
//! 本进程**不初始化**日志 / UI / 音频 / 事件循环（入口在 `main` 最前分支），
//! 只做一件事：跑一次无头检测（或廉价指纹探测）并把结果写到父进程指定的文件。
//!
//! 双保险：
//! - 看门狗线程在语义超时点写一份结构化 `Timeout` 结果并退出——驱动挂起时
//!   主线程卡在 FFI 里，但看门狗线程仍可运行（`process::exit` 是尽力而为：
//!   `ExitProcess` 可能被 loader 锁卡住，此时由父进程 `TerminateProcess` 兜底）；
//! - 正常路径由主线程写结果并退出。
//!
//! 两种模式（父进程参数指定）：
//! - `full`：完整兼容性检测（建实例 / 建设备 / 离屏试画）；
//! - `fingerprints`：只枚举适配器指纹（启动缓存校验，成本远低于全量检测）。
//!   该模式超时**不写任何文件**——空指纹列表会被父进程当作 cache miss，
//!   但不能写成「探测成功但零适配器」误导诊断。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use lumino_gfx::device_check::{self, FallbackDiagnostics, GpuCheckFailure, GpuCheckReport};

use super::{CHILD_FLAG, FALLBACK_FLAG, MODE_FLAG, TIMEOUT_FLAG, parse_fallback, wire};

/// 探针工作模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    /// 完整兼容性检测
    Full,
    /// 仅枚举要求后端的适配器指纹
    Fingerprints,
}

impl Mode {
    /// 命令行取值
    pub(super) const fn as_arg(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Fingerprints => "fingerprints",
        }
    }

    /// 解析命令行取值（未知值返回 `None`，由调用方按协议错误处理）
    pub(super) fn parse(raw: &str) -> Option<Self> {
        match raw {
            "full" => Some(Self::Full),
            "fingerprints" => Some(Self::Fingerprints),
            _ => None,
        }
    }
}

/// 子进程命令行参数
pub(super) struct Args {
    /// 结果文件输出路径（父进程指定）
    pub(super) out: PathBuf,
    /// 工作模式
    pub(super) mode: Mode,
    /// 回退后端诊断收集策略（`fingerprints` 模式忽略）
    pub(super) when: FallbackDiagnostics,
    /// 语义超时（看门狗据此写超时结果）
    pub(super) timeout: Duration,
}

/// 结果写入状态：写锁 + 是否已写。
///
/// 只用 `AtomicBool` 不够：看门狗到点时若主线程**正在写文件**，直接 `exit` 会把
/// 文件截断成半截 JSON。写锁保证「看门狗要么等主线程写完，要么由自己独占写入」。
#[derive(Default)]
struct FinishState {
    /// 是否已有线程完成写入
    written: AtomicBool,
    /// 写入互斥（持有期间进程不会被看门狗杀掉）
    io: Mutex<()>,
}

impl FinishState {
    /// 取写锁（poison 时取回内部值：子进程内 panic 不应让看门狗永久阻塞）
    fn lock_io(&self) -> MutexGuard<'_, ()> {
        match self.io.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// 子进程入口：解析参数 → 起看门狗 → 按模式执行 → 写结果 → 退出。**不返回。**
pub(crate) fn run() -> ! {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            // 协议错误：不写结果文件，父进程按「未回传有效结果」处理
            eprintln!("[device-check-probe] {message}");
            std::process::exit(2);
        }
    };

    let state = Arc::new(FinishState::default());
    spawn_watchdog(&args, Arc::clone(&state));

    match args.mode {
        Mode::Full => {
            let report = device_check::check_gpu_support(args.when);
            write_report_once(&args.out, &state, &report);
        }
        Mode::Fingerprints => {
            // 阻塞式枚举：调用约束见 `probe_adapter_fingerprints` 的文档
            // （生产路径只允许在可强杀的探针子进程内调用）。
            let fingerprints = device_check::probe_adapter_fingerprints();
            write_fingerprints_once(&args.out, &state, &fingerprints);
        }
    }
    std::process::exit(0);
}

/// 看门狗：到点先写超时结果（自杀是优雅路径，父进程强杀是唯一硬保证）。
///
/// 与主线程通过 [`FinishState`] 竞争：主线程正在写时先等它写完；已经写完则直接让
/// 主线程自己退出，不抢先 `exit`。
fn spawn_watchdog(args: &Args, state: Arc<FinishState>) {
    let out = args.out.clone();
    let timeout = args.timeout;
    let mode = args.mode;
    let spawned = std::thread::Builder::new()
        .name("lumino-probe-watchdog".into())
        .spawn(move || {
            std::thread::sleep(timeout);
            {
                let _guard = state.lock_io();
                if state.written.load(Ordering::SeqCst) {
                    // 主线程已完成写入：由它自己退出，避免打断收尾
                    return;
                }
                if mode == Mode::Full {
                    let mut report = GpuCheckReport::failed(GpuCheckFailure::Timeout);
                    report.duration_ms = timeout.as_millis() as u64;
                    if let Err(e) = wire::write_report(&out, &report) {
                        eprintln!("[device-check-probe] 写入超时结果失败: {e}");
                    }
                }
                // fingerprints 模式：超时不写文件（父进程按 cache miss 处理）
                state.written.store(true, Ordering::SeqCst);
            }
            // 检测可能仍卡在驱动里：保证进程退出（父进程还有 TerminateProcess 兜底）
            std::process::exit(0);
        });
    if let Err(e) = spawned {
        // 没有看门狗时仍可继续：父进程硬杀是最后防线
        eprintln!("[device-check-probe] 看门狗线程创建失败: {e}");
    }
}

/// 写完整检测结果（仅第一次生效；持锁期间看门狗不会杀进程）
fn write_report_once(out: &std::path::Path, state: &FinishState, report: &GpuCheckReport) {
    write_once(state, |path| wire::write_report(path, report), out);
}

/// 写廉价探测指纹（仅第一次生效；持锁期间看门狗不会杀进程）
fn write_fingerprints_once(out: &std::path::Path, state: &FinishState, fingerprints: &[String]) {
    write_once(
        state,
        |path| wire::write_fingerprints(path, fingerprints),
        out,
    );
}

/// 写文件的互斥外壳：持锁期间看门狗不会杀进程，避免半截 JSON。
fn write_once(
    state: &FinishState,
    write: impl FnOnce(&std::path::Path) -> std::io::Result<()>,
    out: &std::path::Path,
) {
    let _guard = state.lock_io();
    if state.written.swap(true, Ordering::SeqCst) {
        return;
    }
    if let Err(e) = write(out) {
        eprintln!("[device-check-probe] 写入结果失败: {e}");
    }
}

/// 解析
/// `--device-check-probe <out> --device-check-probe-mode <full|fingerprints>
///  --device-check-fallback <策略> --device-check-timeout-ms <n>`。
fn parse_args() -> Result<Args, String> {
    parse_args_from(std::env::args_os().skip(1))
}

/// 参数解析核心（与 [`super::supervisor`] 的构造约定成对，便于契约测试）。
///
/// 严格模式：未知 `--` 参数直接报错（避免拼写错误静默跑成默认策略），
/// 非 `--` 参数忽略（平台/启动器可能注入无关参数）。
pub(super) fn parse_args_from<I>(args: I) -> Result<Args, String>
where
    I: IntoIterator<Item = std::ffi::OsString>,
{
    let mut out: Option<PathBuf> = None;
    let mut mode: Option<Mode> = None;
    let mut when: Option<FallbackDiagnostics> = None;
    let mut timeout: Option<Duration> = None;

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let Some(flag) = arg.to_str() else {
            return Err("命令行参数不是合法 UTF-8".to_string());
        };
        match flag {
            CHILD_FLAG => {
                out = args.next().map(PathBuf::from);
            }
            MODE_FLAG => {
                mode = args
                    .next()
                    .and_then(|value| value.to_str().and_then(Mode::parse));
            }
            FALLBACK_FLAG => {
                when = args
                    .next()
                    .and_then(|value| value.to_str().and_then(parse_fallback));
            }
            TIMEOUT_FLAG => {
                timeout = args.next().and_then(|value| {
                    value
                        .to_str()
                        .and_then(|value| value.parse::<u64>().ok())
                        .map(Duration::from_millis)
                });
            }
            other if other.starts_with("--") => {
                return Err(format!("未知参数: {other}"));
            }
            _ => {}
        }
    }

    Ok(Args {
        out: out.ok_or_else(|| "缺少结果文件路径".to_string())?,
        mode: mode.ok_or_else(|| "缺少或非法的探针模式".to_string())?,
        when: when.ok_or_else(|| "缺少或非法的回退诊断策略".to_string())?,
        timeout: timeout.ok_or_else(|| "缺少或非法的超时时间".to_string())?,
    })
}
