//! GPU 检测报告类型与平台后端策略。
//!
//! 报告是检查核心与调用方（runner / 设置页）之间唯一的数据契约：
//! 检查核心只产出报告，弹窗、持久化与文案选择均由调用方决定。

/// 单个 GPU 适配器的诊断摘要（供设置页展示与复制）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuAdapterSummary {
    /// 适配器名称
    pub name: String,
    /// 后端名称（Vulkan / Metal / Dx12 / Gl）
    pub backend: String,
    /// 设备类型（DiscreteGpu / IntegratedGpu / Cpu 等）
    pub device_type: String,
    /// 驱动名称
    pub driver: String,
    /// 驱动附加信息
    pub driver_info: String,
}

impl GpuAdapterSummary {
    /// 从 wgpu 适配器信息构造摘要
    pub(super) fn from_info(info: &wgpu::AdapterInfo) -> Self {
        Self {
            name: info.name.clone(),
            backend: format!("{:?}", info.backend),
            device_type: format!("{:?}", info.device_type),
            driver: info.driver.clone(),
            driver_info: info.driver_info.clone(),
        }
    }

    /// 用于启动缓存比对的指纹（后端/名称/驱动任一变化即失效）
    pub fn fingerprint(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}",
            self.backend, self.name, self.driver, self.driver_info, self.device_type
        )
    }

    /// 单行描述（日志与设置页展示用）
    pub fn describe(&self) -> String {
        format!(
            "{} [{}] {} · 驱动 {} {}",
            self.name, self.backend, self.device_type, self.driver, self.driver_info
        )
    }
}

/// 检测失败分类（UI 按类别给出可操作建议）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuCheckFailure {
    /// 未找到本平台要求的图形后端适配器
    NoAdapter,
    /// 找到适配器但请求设备失败（驱动/特性不满足）
    DeviceRequest(String),
    /// 设备创建成功但离屏渲染/回读失败（管线执行异常或白屏）
    RenderError(String),
    /// 检测在超时内未完成（驱动可能异常挂起）
    Timeout,
}

impl std::fmt::Display for GpuCheckFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAdapter => write!(f, "未找到适配器"),
            Self::DeviceRequest(e) => write!(f, "设备请求失败: {e}"),
            Self::RenderError(e) => write!(f, "渲染执行失败: {e}"),
            Self::Timeout => write!(f, "检测超时"),
        }
    }
}

/// GPU 兼容性检测报告
#[derive(Debug, Clone)]
pub struct GpuCheckReport {
    /// 是否通过（等价于 `failure.is_none()`）
    pub passed: bool,
    /// 失败原因（通过时为 `None`）
    pub failure: Option<GpuCheckFailure>,
    /// 本平台要求的后端名称（Vulkan / Metal）
    pub required_backend: &'static str,
    /// 要求后端的适配器列表
    pub adapters: Vec<GpuAdapterSummary>,
    /// 其他后端（DX12/GL 等）的适配器列表，仅用于诊断提示"可能仍可运行"
    pub fallback_adapters: Vec<GpuAdapterSummary>,
    /// 选中适配器指纹（通过时用于启动缓存比对）
    pub fingerprint: Option<String>,
    /// 检测耗时（毫秒）
    pub duration_ms: u64,
}

impl GpuCheckReport {
    /// 构造一份失败报告（不含适配器列表）
    pub fn failed(failure: GpuCheckFailure) -> Self {
        Self {
            passed: false,
            failure: Some(failure),
            required_backend: required_backend_name(),
            adapters: Vec::new(),
            fallback_adapters: Vec::new(),
            fingerprint: None,
            duration_ms: 0,
        }
    }

    /// 汇总诊断文本（日志 / 设置页结果 / 复制诊断信息）
    pub fn detail(&self) -> String {
        let mut out = if self.passed {
            format!("{} 兼容性检测通过", self.required_backend)
        } else {
            match &self.failure {
                Some(failure) => format!("{} 兼容性检测失败: {failure}", self.required_backend),
                None => format!("{} 兼容性检测失败", self.required_backend),
            }
        };
        out.push_str(&format!("（耗时 {}ms）", self.duration_ms));
        if let Some(fingerprint) = &self.fingerprint {
            out.push_str(&format!("\n指纹: {fingerprint}"));
        }
        for adapter in &self.adapters {
            out.push_str(&format!(
                "\n· {} {}",
                self.required_backend,
                adapter.describe()
            ));
        }
        if !self.fallback_adapters.is_empty() {
            out.push_str("\n其他可用后端（回退参考）:");
            for adapter in &self.fallback_adapters {
                out.push_str(&format!("\n· {}", adapter.describe()));
            }
        }
        out
    }
}

/// 本平台要求的图形后端名称（用于文案与诊断）
pub fn required_backend_name() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Metal"
    }
    #[cfg(not(target_os = "macos"))]
    {
        "Vulkan"
    }
}

/// 本平台要求的后端枚举（macOS 无 Vulkan，使用 Metal；用于过滤回退适配器）
pub(super) fn required_backend() -> wgpu::Backend {
    #[cfg(target_os = "macos")]
    {
        wgpu::Backend::Metal
    }
    #[cfg(not(target_os = "macos"))]
    {
        wgpu::Backend::Vulkan
    }
}

/// 本平台要求的后端集合（用于创建检测实例）
pub(super) fn required_backends() -> wgpu::Backends {
    #[cfg(target_os = "macos")]
    {
        wgpu::Backends::METAL
    }
    #[cfg(not(target_os = "macos"))]
    {
        wgpu::Backends::VULKAN
    }
}
