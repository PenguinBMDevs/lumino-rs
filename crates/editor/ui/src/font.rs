//! 程序 UI 字体绑定（UI-001 的 egui 版）。
//!
//! 设置面板把用户选择写入 `program_font_name` / `program_font_path`，本模块负责
//! 把「名称或路径」解析成字体文件字节，并交给 [`lumino_ui_egui::fonts::configure`]
//! 登记为进程级字体策略。
//!
//! # 优先级与失败策略
//!
//! 1. 自定义字体文件路径（非空且能加载）；
//! 2. 系统字体名（字体文件内的全部本地化家族名均可命中，不区分大小写）；
//! 3. 全部失败 → 回退内置默认字体，只告警不崩溃（UI-001 契约）。
//!
//! # 调用点
//!
//! - 启动：`WindowManager::new`，早于任何 egui `Context` 创建；
//! - 设置应用：`Host::apply_settings`，字体代数变化后各 `Context` 下一帧重建
//!   字体表，因此修改**立即生效**，无需重启。

use std::sync::Arc;

use lumino_core::storage::config::UiConfig;
use lumino_ui_egui::fonts::ProgramFont;

/// 按「名称 + 路径」应用程序字体。
///
/// 路径优先于名称；前者加载失败时继续尝试名称（配置层虽然互斥，但手工输入
/// 或旧配置可能同时存在两者）。两者都不可用时回退内置默认字体。
pub fn apply(program_font_name: &str, program_font_path: &str) {
    for source in [program_font_path.trim(), program_font_name.trim()] {
        if source.is_empty() {
            continue;
        }
        match lumino_note_core::font_scanner::load_font_face(source) {
            Some(face) => {
                let font = ProgramFont {
                    source: source.to_owned(),
                    family: face.family,
                    bytes: Arc::new(face.bytes),
                    face_index: face.index,
                };
                lumino_ui_egui::fonts::configure(Some(font));
                return;
            }
            None => tracing::warn!("程序字体无法加载，尝试下一来源: '{source}'"),
        }
    }
    // 名称与路径均为空，或全部解析失败：回退内置默认字体。
    lumino_ui_egui::fonts::configure(None);
}

/// 按启动配置应用程序字体（保留给需要在启动期同步解析的调用方）。
///
/// 系统字体名首次解析会枚举系统字体（Windows 约 1.3s）。启动路径如需避免
/// 阻塞首帧，请先尝试自定义文件路径，再把名称解析放进后台线程。
pub fn apply_from_config(config: &UiConfig) {
    apply(&config.program_font_name, &config.program_font_path);
}

/// 当前已登记的程序字体来源（诊断 / 启动快速路径判断）。
pub fn configured_source() -> Option<String> {
    lumino_ui_egui::fonts::configured_source()
}

/// 启动期后台解析系统字体名专用。
///
/// 系统字体枚举较慢（Windows 约 1.3s），启动路径把它放到后台线程。解析完成后
/// 若用户已在设置里改过字体（全局策略已有值），丢弃本次结果，避免旧配置覆盖
/// 新选择。
pub fn apply_startup_name(program_font_name: &str) {
    let name = program_font_name.trim();
    if name.is_empty() {
        return;
    }
    let Some(face) = lumino_note_core::font_scanner::load_font_face(name) else {
        tracing::warn!("启动字体无法加载，回退默认字体: '{name}'");
        return;
    };
    if configured_source().is_some() {
        tracing::info!("启动字体解析完成，但字体已被用户修改，忽略旧配置: '{name}'");
        return;
    }
    lumino_ui_egui::fonts::configure(Some(ProgramFont {
        source: name.to_owned(),
        family: face.family,
        bytes: Arc::new(face.bytes),
        face_index: face.index,
    }));
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Mutex;

    use super::*;

    /// 全局字体策略是进程级状态：同进程并行测试必须串行化
    /// （`cargo nextest` 一进程一用例，本就隔离）。
    static FONT_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn font_test_lock() -> std::sync::MutexGuard<'static, ()> {
        FONT_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn bundled_font_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../resources/fonts/NotoSansSC-Regular.ttf")
    }

    #[test]
    fn test_apply_custom_font_path_configures_program_font() {
        let _guard = font_test_lock();
        let path = bundled_font_path();
        assert!(path.is_file(), "测试字体必须存在: {}", path.display());
        let source = path.to_string_lossy();

        apply("", &source);
        assert_eq!(
            lumino_ui_egui::fonts::configured_source().as_deref(),
            Some(source.as_ref()),
            "自定义字体路径必须登记为程序字体"
        );

        // 恢复默认，避免同进程内其他用例受影响。
        lumino_ui_egui::fonts::configure(None);
    }

    #[test]
    fn test_apply_invalid_sources_fall_back_to_default() {
        let _guard = font_test_lock();
        apply("NoSuchFontXYZ", "C:/definitely/not/exist.ttf");
        assert_eq!(
            lumino_ui_egui::fonts::configured_source(),
            None,
            "无效来源必须回退默认字体"
        );
    }

    #[test]
    fn test_apply_from_config_prefers_path() {
        let _guard = font_test_lock();
        let path = bundled_font_path();
        let source = path.to_string_lossy().into_owned();
        let config = UiConfig {
            program_font_name: "NoSuchFontXYZ".to_owned(),
            program_font_path: source.clone(),
            ..UiConfig::default()
        };

        apply_from_config(&config);
        assert_eq!(
            lumino_ui_egui::fonts::configured_source(),
            Some(source),
            "路径必须优先于系统字体名"
        );

        lumino_ui_egui::fonts::configure(None);
    }
}
