//! 探针 IPC 数据契约（父 / 子进程共用）。
//!
//! 刻意**不复用** `lumino_gfx::device_check` 的内部类型：IPC 是跨进程协议，
//! 直接序列化内部结构意味着任何字段调整都会变成协议破坏。这里只保留
//! 「父进程重建报告所需的最小信息」，并带协议版本号——版本不匹配按
//! 「检测进程异常」处理，而不是拿半截数据当结论。

use std::io::Write;
use std::path::Path;

use lumino_core::trf;
use lumino_gfx::device_check::{
    GpuAdapterSummary, GpuCheckFailure, GpuCheckReport, required_backend_name,
};
use serde::{Deserialize, Serialize};

/// 协议版本：字段或语义变化必须递增
pub(crate) const PROTOCOL_VERSION: u32 = 1;

/// 子进程回传的检测报告
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ProbeReport {
    /// 协议版本
    pub(crate) version: u32,
    /// 是否通过
    pub(crate) passed: bool,
    /// 失败原因（通过时缺省）
    pub(crate) failure: Option<ProbeFailure>,
    /// 要求后端的适配器列表
    pub(crate) adapters: Vec<ProbeAdapter>,
    /// 回退后端适配器列表（诊断用）
    #[serde(default)]
    pub(crate) fallback_adapters: Vec<ProbeAdapter>,
    /// 选中适配器指纹
    pub(crate) fingerprint: Option<String>,
    /// 检测耗时（毫秒）
    pub(crate) duration_ms: u64,
}

/// 失败分类（与 [`GpuCheckFailure`] 一一对应，去掉本地化 Display）
#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum ProbeFailure {
    /// 未找到适配器
    NoAdapter,
    /// 设备请求失败
    DeviceRequest(String),
    /// 渲染执行失败
    RenderError(String),
    /// 超时
    Timeout,
    /// 执行环境异常
    Aborted(String),
}

/// 适配器摘要（与 [`GpuAdapterSummary`] 一一对应）
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ProbeAdapter {
    /// 适配器名称
    pub(crate) name: String,
    /// 后端名称
    pub(crate) backend: String,
    /// 设备类型
    pub(crate) device_type: String,
    /// 驱动名称
    pub(crate) driver: String,
    /// 驱动附加信息
    pub(crate) driver_info: String,
}

impl From<&GpuCheckFailure> for ProbeFailure {
    fn from(failure: &GpuCheckFailure) -> Self {
        match failure {
            GpuCheckFailure::NoAdapter => Self::NoAdapter,
            GpuCheckFailure::DeviceRequest(message) => Self::DeviceRequest(message.clone()),
            GpuCheckFailure::RenderError(message) => Self::RenderError(message.clone()),
            GpuCheckFailure::Timeout => Self::Timeout,
            GpuCheckFailure::Aborted(message) => Self::Aborted(message.clone()),
        }
    }
}

impl From<ProbeFailure> for GpuCheckFailure {
    fn from(failure: ProbeFailure) -> Self {
        match failure {
            ProbeFailure::NoAdapter => Self::NoAdapter,
            ProbeFailure::DeviceRequest(message) => Self::DeviceRequest(message),
            ProbeFailure::RenderError(message) => Self::RenderError(message),
            ProbeFailure::Timeout => Self::Timeout,
            ProbeFailure::Aborted(message) => Self::Aborted(message),
        }
    }
}

impl From<&GpuAdapterSummary> for ProbeAdapter {
    fn from(adapter: &GpuAdapterSummary) -> Self {
        Self {
            name: adapter.name.clone(),
            backend: adapter.backend.clone(),
            device_type: adapter.device_type.clone(),
            driver: adapter.driver.clone(),
            driver_info: adapter.driver_info.clone(),
        }
    }
}

impl From<ProbeAdapter> for GpuAdapterSummary {
    fn from(adapter: ProbeAdapter) -> Self {
        Self {
            name: adapter.name,
            backend: adapter.backend,
            device_type: adapter.device_type,
            driver: adapter.driver,
            driver_info: adapter.driver_info,
        }
    }
}

impl From<&GpuCheckReport> for ProbeReport {
    fn from(report: &GpuCheckReport) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            passed: report.passed,
            failure: report.failure.as_ref().map(ProbeFailure::from),
            adapters: report.adapters.iter().map(ProbeAdapter::from).collect(),
            fallback_adapters: report
                .fallback_adapters
                .iter()
                .map(ProbeAdapter::from)
                .collect(),
            fingerprint: report.fingerprint.clone(),
            duration_ms: report.duration_ms,
        }
    }
}

impl TryFrom<ProbeReport> for GpuCheckReport {
    type Error = String;

    fn try_from(wire: ProbeReport) -> Result<Self, Self::Error> {
        if wire.version != PROTOCOL_VERSION {
            return Err(trf!(
                "协议版本不匹配（子进程 {got}，当前 {want}）",
                got = wire.version,
                want = PROTOCOL_VERSION
            ));
        }
        Ok(Self {
            passed: wire.passed,
            failure: wire.failure.map(GpuCheckFailure::from),
            required_backend: required_backend_name(),
            adapters: wire
                .adapters
                .into_iter()
                .map(GpuAdapterSummary::from)
                .collect(),
            fallback_adapters: wire
                .fallback_adapters
                .into_iter()
                .map(GpuAdapterSummary::from)
                .collect(),
            fingerprint: wire.fingerprint,
            duration_ms: wire.duration_ms,
        })
    }
}

/// 原子写入结果：先写 `.tmp` 再改名，父进程读到的永远是完整 JSON。
pub(crate) fn write_report(path: &Path, report: &GpuCheckReport) -> std::io::Result<()> {
    let payload = serde_json::to_vec(&ProbeReport::from(report))
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension("tmp");
    {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(&payload)?;
        // 先落盘再改名：避免父进程在改名瞬间读到空文件
        file.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

/// 读取并校验子进程回传的结果。
pub(crate) fn read_report(path: &Path) -> Result<GpuCheckReport, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| trf!("读取结果文件失败: {e}", e = e))?;
    let parsed: ProbeReport =
        serde_json::from_str(&raw).map_err(|e| trf!("结果 JSON 解析失败: {e}", e = e))?;
    GpuCheckReport::try_from(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_adapter() -> GpuAdapterSummary {
        GpuAdapterSummary {
            name: "NVIDIA GeForce RTX 2060".into(),
            backend: "Vulkan".into(),
            device_type: "DiscreteGpu".into(),
            driver: "NVIDIA".into(),
            driver_info: "610.62".into(),
        }
    }

    fn sample_report() -> GpuCheckReport {
        GpuCheckReport {
            passed: false,
            failure: Some(GpuCheckFailure::Timeout),
            required_backend: required_backend_name(),
            adapters: vec![sample_adapter()],
            fallback_adapters: Vec::new(),
            fingerprint: Some("Vulkan|RTX|NVIDIA|610.62|DiscreteGpu".into()),
            duration_ms: 3000,
        }
    }

    #[test]
    fn test_report_round_trips_through_json() {
        let original = sample_report();
        let encoded = serde_json::to_string(&ProbeReport::from(&original)).expect("序列化失败");
        let decoded: ProbeReport = serde_json::from_str(&encoded).expect("反序列化失败");
        let restored = GpuCheckReport::try_from(decoded).expect("协议校验失败");

        assert_eq!(restored.passed, original.passed);
        assert_eq!(restored.failure, original.failure);
        assert_eq!(restored.fingerprint, original.fingerprint);
        assert_eq!(restored.duration_ms, original.duration_ms);
        assert_eq!(restored.adapters, original.adapters);
        assert_eq!(restored.required_backend, required_backend_name());
    }

    /// 协议版本不匹配必须拒绝，而不是把字段当结论用
    #[test]
    fn test_protocol_version_mismatch_is_rejected() {
        let mut wire = ProbeReport::from(&sample_report());
        wire.version = PROTOCOL_VERSION + 1;
        let err = GpuCheckReport::try_from(wire).expect_err("版本不匹配必须报错");
        assert!(
            err.contains("协议版本不匹配"),
            "错误信息应指明版本问题: {err}"
        );
    }

    #[test]
    fn test_write_then_read_round_trip() {
        let path = std::env::temp_dir().join(format!(
            "lumino-probe-wire-test-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let report = sample_report();
        write_report(&path, &report).expect("写入失败");
        let loaded = read_report(&path).expect("读取失败");
        assert_eq!(loaded.failure, report.failure);
        assert_eq!(loaded.adapters.len(), 1);
        assert!(!loaded.passed);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_garbage_payload_is_rejected() {
        let path = std::env::temp_dir().join(format!(
            "lumino-probe-wire-garbage-{}.json",
            std::process::id()
        ));
        std::fs::write(&path, b"not a json").expect("写临时文件失败");
        let err = read_report(&path).expect_err("垃圾数据必须报错");
        assert!(err.contains("解析失败"), "错误信息应指明解析失败: {err}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_missing_file_is_rejected() {
        let path = std::env::temp_dir().join(format!(
            "lumino-probe-wire-missing-{}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        assert!(read_report(&path).is_err());
    }
}
