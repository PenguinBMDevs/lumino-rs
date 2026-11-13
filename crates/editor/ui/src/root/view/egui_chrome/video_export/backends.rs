//! 视频导出面板用到的平台相关常量（原在 旧版 视图 `helpers` / `counter_settings_font`）。
//!
//! 旧版 设置视图删除后，这里是唯一来源；`ui-egui` 不依赖 旧版，只吃纯数据。

/// 计数器 / 数据曲线可选系统字体族（**数据值**，用于按名查字体路径，不翻译）。
pub(crate) const SYSTEM_FONT_FAMILIES: [&str; 8] = [
    "微软雅黑",
    "微软雅黑粗体",
    "宋体",
    "黑体",
    "楷体",
    "仿宋",
    "Arial",
    "Consolas",
];

/// 可用硬件加速后端列表（首个恒为软件编码）。
pub(crate) fn available_backends() -> Vec<String> {
    let mut list = vec!["Software (CPU)".to_string()];
    #[cfg(target_os = "macos")]
    list.push("VideoToolbox (Apple)".to_string());
    #[cfg(target_os = "windows")]
    {
        list.push("NVENC (NVIDIA)".to_string());
        list.push("AMF (AMD)".to_string());
        list.push("QSV (Intel)".to_string());
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        list.push("VAAPI (Linux)".to_string());
        list.push("NVENC (NVIDIA)".to_string());
    }
    list
}
