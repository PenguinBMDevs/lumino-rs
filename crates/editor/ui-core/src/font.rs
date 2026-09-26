//! 全局 UI 字体入口（UI-001）
//!
//! 所有 UI 组件（widget 与 canvas 绘制）统一从本模块取字体，消灭组件级硬编码；
//! 启动时由 [`init`] 解析一次（字体切换重启生效），此后各处调用 [`ui_font`]。
//!
//! 解析顺序（与设置面板语义一致，path 有效优先）：
//! 1. `program_font_path` 有效 → 加载字体文件（注册进 iced 字体库）并按文件家族名引用；
//! 2. `program_font_name` 非空 → 按系统字体名引用；
//! 3. 否则 → iced 默认字体（SansSerif）。
//!
//! 任一步失败仅告警并降级（回退下一优先级），不 panic、不崩溃。
//!
//! 说明（CJK 回退）：iced 段落文本对非 ASCII 自动使用 cosmic-text `Advanced` shaping，
//! 缺字形时按平台字体链逐字形回退（实测：用户字体无 CJK 时中文自动回退到系统 CJK 字体，
//! 西文/数字保持用户字体）；canvas 绘制需显式指定 `Shaping::Advanced` 才启用回退。

use std::{
    borrow::Cow,
    collections::HashMap,
    path::Path,
    sync::{Mutex, OnceLock},
};

use iced_core::Font;
use lumino_core::storage::config::UiConfig;

/// 全局 UI 字体（启动时解析一次）
static UI_FONT: OnceLock<Font> = OnceLock::new();

/// 字体名泄漏缓存（`Font::with_name` 需要 `'static` 名称；按名称去重，避免重复泄漏）
static LEAKED_NAMES: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();

/// 解析字体（纯函数，便于测试）
///
/// 优先级：`program_font_path`（有效文件）→ `program_font_name` → 默认字体。
pub fn resolve_font(ui_config: &UiConfig) -> Font {
    // 1) 自定义字体文件路径优先
    if !ui_config.program_font_path.is_empty() {
        let path = Path::new(&ui_config.program_font_path);
        if path.is_file() {
            match load_font_file(path) {
                Some(font) => return font,
                None => tracing::warn!("自定义字体文件解析失败，回退到字体名称: {:?}", path),
            }
        } else {
            tracing::warn!("自定义字体路径不存在，回退到字体名称: {:?}", path);
        }
    }

    // 2) 系统字体名称
    if !ui_config.program_font_name.is_empty() {
        tracing::info!("应用字体: {}", ui_config.program_font_name);
        return font_by_name(&ui_config.program_font_name);
    }

    // 3) 默认字体
    tracing::info!("使用默认字体 (SansSerif)");
    Font::default()
}

/// 启动时初始化全局字体入口（幂等；重复调用返回首次解析结果）
///
/// 由主窗口构建流程调用；各对话框/进度窗口共享同一字体（字体切换重启生效）。
pub fn init(ui_config: &UiConfig) -> Font {
    *UI_FONT.get_or_init(|| resolve_font(ui_config))
}

/// 全局 UI 字体
///
/// 未初始化时回退 iced 默认字体（不 panic）；渲染器默认字体与之一致。
pub fn ui_font() -> Font {
    UI_FONT.get().copied().unwrap_or_default()
}

/// 全局 UI 字体（粗体变体；保留用户字体 family）
pub fn ui_font_bold() -> Font {
    Font {
        weight: iced_core::font::Weight::Bold,
        ..ui_font()
    }
}

/// 加载字体文件并注册进 iced 字体库，返回按其家族名构造的 [`Font`]
///
/// 失败（读文件/解析家族名/字体库加锁失败）返回 `None`，由调用方降级。
fn load_font_file(path: &Path) -> Option<Font> {
    let bytes = std::fs::read(path).ok()?;
    let family = lumino_note_core::font_scanner::resolve_font_family(path)?;

    {
        let mut font_system = iced_wgpu::graphics::text::font_system().write().ok()?;
        font_system.load_font(Cow::Owned(bytes));
    }

    tracing::info!("已加载自定义字体文件: {:?}（family={}）", path, family);
    Some(font_by_name(&family))
}

/// 按名称构造 [`Font`]（名称经缓存泄漏为 `'static`，重复调用不重复泄漏）
fn font_by_name(name: &str) -> Font {
    let cache = LEAKED_NAMES.get_or_init(|| Mutex::new(HashMap::new()));
    let leaked = {
        let mut map = cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *map.entry(name.to_string())
            .or_insert_with(|| Box::leak(name.to_string().into_boxed_str()))
    };
    Font::with_name(leaked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_core::storage::config::UiConfig;

    fn config_with(name: &str, path: &str) -> UiConfig {
        UiConfig {
            program_font_name: name.to_string(),
            program_font_path: path.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_resolve_font_empty_config_uses_default() {
        let font = resolve_font(&config_with("", ""));
        assert_eq!(font, Font::default());
    }

    #[test]
    fn test_resolve_font_by_name() {
        let font = resolve_font(&config_with("TestFontXYZ", ""));
        assert_eq!(font, Font::with_name("TestFontXYZ"));
    }

    #[test]
    fn test_resolve_font_invalid_path_falls_back_to_name() {
        let font = resolve_font(&config_with(
            "FallbackFontXYZ",
            "C:/definitely/not/exist/font.ttf",
        ));
        assert_eq!(font, Font::with_name("FallbackFontXYZ"));
    }

    #[test]
    fn test_resolve_font_path_takes_priority() {
        // 取系统缓存中的字体文件：path 有效时应优先于 name
        let Some(font_info) = lumino_note_core::font_scanner::get_cached_fonts().first() else {
            println!("系统无可用字体，跳过");
            return;
        };
        let path = font_info.path.to_string_lossy().into_owned();
        let font = resolve_font(&config_with("ShouldBeIgnoredFont", &path));
        let expected_family = lumino_note_core::font_scanner::resolve_font_family(&font_info.path);
        match expected_family {
            Some(family) => match font.family {
                iced_core::font::Family::Name(name) => assert_eq!(name, family),
                other => panic!("期望按家族名引用，实际: {other:?}"),
            },
            None => println!("字体 family 解析失败，跳过: {path}"),
        }
    }

    /// 回归护栏（UI-001 实测结论）：
    /// 无 CJK 覆盖的用户字体 + `Shaping::Advanced`（iced 段落对非 ASCII 的默认路径）
    /// 必须逐字形回退到系统 CJK 字体，不得出现 .notdef 豆腐块。
    #[test]
    fn test_shaping_cjk_fallback_with_advanced() {
        use cosmic_text::{Attrs, Buffer, Family, Metrics, Shaping};
        use iced_wgpu::graphics::text::{cosmic_text, font_system};

        // 1) 找一个"无 CJK 覆盖"的西文字体（存在才测）
        let latin_font = ["Arial", "Calibri", "DejaVu Sans", "Liberation Sans"]
            .into_iter()
            .find(|name| {
                lumino_note_core::font_scanner::get_cached_fonts()
                    .iter()
                    .any(|f| f.name.eq_ignore_ascii_case(name))
                    && !lumino_note_core::font_scanner::font_supports_cjk(name)
            });
        let Some(latin_font) = latin_font else {
            println!("未找到无 CJK 覆盖的西文字体，跳过");
            return;
        };
        // 2) 系统需有 CJK 字体可回退
        if lumino_note_core::font_scanner::pick_cjk_font().is_none() {
            println!("系统未找到 CJK 字体，跳过");
            return;
        }

        let text = "中文测试 ABC 123";
        let mut guard = font_system().write().expect("font system lock");
        let fs = guard.raw();
        let mut buffer = Buffer::new(fs, Metrics::new(16.0, 20.0));
        buffer.set_size(fs, Some(600.0), Some(80.0));
        buffer.set_text(
            fs,
            text,
            &Attrs::new().family(Family::Name(latin_font)),
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(fs, false);

        let mut notdef = 0usize;
        let mut primary_font = None;
        let mut fallback_used = false;
        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                if glyph.glyph_id == 0 {
                    notdef += 1;
                }
                match primary_font {
                    None => primary_font = Some(glyph.font_id),
                    Some(primary) if primary != glyph.font_id => fallback_used = true,
                    _ => {}
                }
            }
        }
        assert_eq!(notdef, 0, "不应出现豆腐块（字体={latin_font}）");
        assert!(
            fallback_used,
            "中文应回退到系统 CJK 字体（字体={latin_font}）"
        );
    }
}
