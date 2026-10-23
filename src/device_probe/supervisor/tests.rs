//! 探针监督器单元测试（假子进程 + 父子参数契约）。

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

    let report = run_report_command(cmd, out, Duration::from_secs(2));

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
    let report = run_report_command(cmd, out, Duration::from_secs(2));

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
    let report = run_report_command(cmd, out, Duration::from_secs(2));

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
        Mode::Full,
        FallbackDiagnostics::OnFailure,
    );
    let parsed =
        super::super::child::parse_args_from(args).expect("子进程必须能解析父进程构造的参数");

    assert_eq!(parsed.out, out);
    assert_eq!(parsed.mode, Mode::Full);
    assert_eq!(parsed.when, FallbackDiagnostics::OnFailure);
    assert_eq!(parsed.timeout, Duration::from_millis(3000));

    // fingerprints 模式的参数同样必须被解析（模式走错 = 探测拿到全量报告）
    let args = child_args(
        &PathBuf::from("probe-out.json"),
        Duration::from_millis(1500),
        Mode::Fingerprints,
        FallbackDiagnostics::Never,
    );
    let parsed = super::super::child::parse_args_from(args).expect("指纹模式参数必须可解析");
    assert_eq!(parsed.mode, Mode::Fingerprints);
}

/// 廉价探测：子进程回传的指纹必须原样读回
#[test]
fn test_fingerprints_are_read_back() {
    let out = unique_out("fp-ok");
    let expected = vec![
        "Vulkan|RTX|NVIDIA|610.62|DiscreteGpu".to_string(),
        "Vulkan|Intel|Intel|1.0|IntegratedGpu".to_string(),
    ];
    wire::write_fingerprints(&out, &expected).expect("预置指纹失败");

    let mut cmd = immediate_exit_command();
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    let result = run_fingerprints_command(cmd, out, Duration::from_secs(2));

    assert_eq!(result, Some(expected), "指纹必须原样回传");
}

/// 廉价探测超时 → `None`（cache miss），不得返回空列表冒充「探测成功」
#[test]
fn test_fingerprints_timeout_is_cache_miss() {
    let out = unique_out("fp-hang");
    let _ = std::fs::remove_file(&out);
    let mut cmd = sleeping_command();
    cmd.stdout(Stdio::null()).stderr(Stdio::null());

    let started = Instant::now();
    let result = run_fingerprints_command(cmd, out, Duration::from_millis(200));

    assert_eq!(result, None, "超时必须折算为 cache miss");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "超时强杀不应额外等待（实际 {:?}）",
        started.elapsed()
    );
}

/// 廉价探测未写出结果（崩溃 / 协议错误）→ `None`
#[test]
fn test_fingerprints_missing_file_is_cache_miss() {
    let out = unique_out("fp-missing");
    let _ = std::fs::remove_file(&out);
    let mut cmd = immediate_exit_command();
    cmd.stdout(Stdio::null()).stderr(Stdio::null());

    assert_eq!(
        run_fingerprints_command(cmd, out, Duration::from_secs(2)),
        None
    );
}

/// 未知 `--` 参数必须报错（避免拼写错误静默跑成默认策略）
#[test]
fn test_child_parser_rejects_unknown_flag() {
    let args = vec![OsString::from("--device-check-prob"), OsString::from("x")];
    assert!(super::super::child::parse_args_from(args).is_err());
}
