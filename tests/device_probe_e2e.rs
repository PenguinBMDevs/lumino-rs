//! GPU 检测探针子进程的端到端测试（真实可执行文件 + 真实 IPC 契约）。
//!
//! 单元测试只能覆盖「监督器 + 协议解析」；「真实二进制 → 参数解析 → 检测 →
//! 写回 JSON」这条链路必须在二进制上验证。这里不依赖 GPU 结论：无 GPU / 驱动异常时
//! 子进程同样会写一份合法报告，契约依然成立。
//!
//! 挂起注入测试依赖 `LUMINO_DEBUG_CHECK_HANG`（仅 debug 构建生效），
//! release 下自动跳过——它验证的是「看门狗在驱动挂起时仍能落一份 Timeout 结果」。

use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// 被测可执行文件（cargo 为集成测试注入）
const BIN: &str = env!("CARGO_BIN_EXE_lumino-rs");

fn unique_path(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "lumino-probe-e2e-{tag}-{}-{nanos}.json",
        std::process::id()
    ))
}

/// 构造探针子进程命令（与 supervisor 生产路径同一套参数）
fn probe_command(out: &Path, timeout_ms: u64, mode: &str) -> Command {
    let mut cmd = Command::new(BIN);
    cmd.arg("--device-check-probe")
        .arg(out)
        .arg("--device-check-probe-mode")
        .arg(mode)
        .arg("--device-check-fallback")
        .arg("always")
        .arg("--device-check-timeout-ms")
        .arg(timeout_ms.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // debug 构建是 console 子系统：不抑制会闪黑框
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// 等子进程结束；超时则强杀并让断言失败（避免测试挂死）
fn wait_with_deadline(child: &mut Child, limit: Duration) -> ExitStatus {
    let started = Instant::now();
    loop {
        match child.try_wait().expect("读取子进程状态失败") {
            Some(status) => return status,
            None if started.elapsed() >= limit => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("探针子进程在 {limit:?} 内未结束");
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

fn read_json(path: &Path) -> serde_json::Value {
    let raw = std::fs::read_to_string(path).expect("探针未写出结果文件");
    serde_json::from_str(&raw).expect("探针结果不是合法 JSON")
}

/// 正常路径：真实二进制必须写出 v1 协议报告
#[test]
fn test_probe_child_writes_valid_report() {
    let out = unique_path("ok");
    let _ = std::fs::remove_file(&out);

    let mut child = probe_command(&out, 10_000, "full")
        .spawn()
        .expect("探针子进程应能启动");
    let status = wait_with_deadline(&mut child, Duration::from_secs(60));
    assert!(status.success(), "探针应正常退出，实际 {status}");

    let json = read_json(&out);
    assert_eq!(json["version"], 1, "协议版本必须为 1");
    assert!(
        json["passed"].is_boolean(),
        "报告必须带布尔 passed 字段: {json}"
    );
    if let Some(failure) = json["failure"].as_str() {
        assert!(
            [
                "NoAdapter",
                "DeviceRequest",
                "RenderError",
                "Timeout",
                "Aborted"
            ]
            .contains(&failure),
            "失败分类必须属于已知集合: {failure}"
        );
    }

    let _ = std::fs::remove_file(&out);
}

/// 挂起注入：看门狗必须在超时点写出结构化 Timeout 并退出
#[cfg(debug_assertions)]
#[test]
fn test_probe_watchdog_reports_timeout_on_injected_hang() {
    let out = unique_path("hang");
    let _ = std::fs::remove_file(&out);

    let started = Instant::now();
    let mut cmd = probe_command(&out, 1500, "full");
    cmd.env("LUMINO_DEBUG_CHECK_HANG", "1");
    let mut child = cmd.spawn().expect("探针子进程应能启动");
    let status = wait_with_deadline(&mut child, Duration::from_secs(30));
    let elapsed = started.elapsed();

    assert!(status.success(), "看门狗路径应正常退出，实际 {status}");
    assert!(
        elapsed < Duration::from_secs(20),
        "挂起注入下必须靠看门狗及时退出，实际 {elapsed:?}"
    );

    let json = read_json(&out);
    assert_eq!(json["failure"], "Timeout", "挂起必须落成 Timeout: {json}");
    assert_eq!(json["passed"], false);

    let _ = std::fs::remove_file(&out);
}

/// 廉价探测模式：真实二进制只枚举指纹，不建 wgpu 设备
#[test]
fn test_probe_fingerprints_mode_writes_list() {
    let out = unique_path("fp");
    let _ = std::fs::remove_file(&out);

    let mut child = probe_command(&out, 10_000, "fingerprints")
        .spawn()
        .expect("探针子进程应能启动");
    let status = wait_with_deadline(&mut child, Duration::from_secs(60));
    assert!(status.success(), "指纹模式应正常退出，实际 {status}");

    let json = read_json(&out);
    assert_eq!(json["version"], 1, "协议版本必须为 1");
    assert!(
        json["fingerprints"].is_array(),
        "必须回传指纹数组（无 GPU 时为空数组）: {json}"
    );

    let _ = std::fs::remove_file(&out);
}

/// 指纹模式挂起：看门狗只退出、**不写任何文件**（父进程按 cache miss 处理）
#[cfg(debug_assertions)]
#[test]
fn test_probe_fingerprints_hang_writes_nothing() {
    let out = unique_path("fp-hang");
    let _ = std::fs::remove_file(&out);

    let mut cmd = probe_command(&out, 1500, "fingerprints");
    cmd.env("LUMINO_DEBUG_PROBE_HANG", "1");
    let mut child = cmd.spawn().expect("探针子进程应能启动");
    let status = wait_with_deadline(&mut child, Duration::from_secs(30));

    assert!(status.success(), "看门狗路径应正常退出，实际 {status}");
    assert!(
        !out.exists(),
        "指纹模式超时不得写出文件（空列表会被误读成「探测成功」）"
    );
}

/// 参数缺失 → 协议错误退出码 2（不得静默跑成默认策略）
#[test]
fn test_probe_child_rejects_missing_arguments() {
    let mut cmd = Command::new(BIN);
    cmd.arg("--device-check-probe")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd.spawn().expect("探针子进程应能启动");
    let status = wait_with_deadline(&mut child, Duration::from_secs(30));
    assert_eq!(status.code(), Some(2), "参数协议错误必须以 2 退出");
}
