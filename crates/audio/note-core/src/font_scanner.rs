//! 跨平台字体扫描模块
//!
//! 使用 font-kit 调用系统 API 获取准确的字体信息。
//!
//! 字体扫描是重操作（Windows 上枚举 200-500+ 字体），
//! 通过全局 OnceLock 缓存避免每次对话框重建时重复扫描约 1.3s 的延迟。

use std::path::PathBuf;
use std::sync::OnceLock;

/// 字体信息
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontInfo {
    /// 字体名称（从系统 API 获取的真实名称）
    pub name: String,
    /// 字体文件路径
    pub path: PathBuf,
}

impl std::fmt::Display for FontInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

/// 扫描系统字体，返回可用字体列表
///
/// - macOS：`font-kit` 的 CoreText 后端在非主线程返回空且 `Handle::Memory` 无路径，
///   此处改用 `fontdb` 直接扫描文件系统（`/System/Library/Fonts` 等），与 `cosmic-text`
///   同源，可在任意线程调用且返回真实路径
/// - 其他平台：`font-kit` SystemSource 已在 Windows/Linux 上准确且路径为 `Handle::Path`
#[cfg(target_os = "macos")]
pub fn scan_system_fonts() -> Vec<FontInfo> {
    use std::collections::HashMap;

    let mut db = fontdb::Database::new();
    db.load_system_fonts();

    // 以家族名为 key 去重，每族保留首次出现的路径（下拉框 per-family）
    let mut map: HashMap<String, PathBuf> = HashMap::new();
    for face in db.faces() {
        let Some((family, _)) = face.families.first() else {
            continue;
        };
        if family.trim().is_empty() || map.contains_key(family) {
            continue;
        }
        let path = match &face.source {
            fontdb::Source::File(p) => p.clone(),
            fontdb::Source::SharedFile(p, _) => p.clone(),
            fontdb::Source::Binary(_) => continue,
        };
        map.insert(family.clone(), path);
    }

    let mut fonts: Vec<FontInfo> = map
        .into_iter()
        .map(|(name, path)| FontInfo { name, path })
        .collect();

    fonts.sort_by_key(|a| a.name.to_lowercase());
    fonts
}

/// 扫描系统字体，返回可用字体列表
///
/// 使用 font-kit 的 SystemSource 调用系统 API 获取准确的字体信息，
/// 包括真实的字体名称（而非文件名）和字体文件路径。
#[cfg(not(target_os = "macos"))]
pub fn scan_system_fonts() -> Vec<FontInfo> {
    use font_kit::source::SystemSource;

    let source = SystemSource::new();

    // 获取所有字体家族名称
    let family_names = match source.all_families() {
        Ok(names) => names,
        Err(e) => {
            tracing::warn!("Failed to get font families: {}", e);
            return Vec::new();
        }
    };

    let mut fonts: Vec<FontInfo> = family_names
        .into_iter()
        .filter_map(|family_name| {
            // 获取字体句柄
            let font = source.select_family_by_name(&family_name).ok()?;

            // 获取字体家族中的第一个字体
            let font_handle = font.fonts().first()?.clone();

            // 加载字体以获取路径
            let font_ref = font_handle.load().ok()?;

            // 获取字体文件路径
            font_ref.copy_font_data()?;
            let path = match &font_handle {
                font_kit::handle::Handle::Path { path, .. } => path.clone(),
                font_kit::handle::Handle::Memory { .. } => PathBuf::new(),
            };

            // 跳过内存字体（无实际路径）
            if path.as_os_str().is_empty() {
                return None;
            }

            Some(FontInfo {
                name: family_name,
                path,
            })
        })
        .collect();

    // 按字体名称排序（不区分大小写）
    fonts.sort_by_key(|a| a.name.to_lowercase());

    fonts
}

/// 全局缓存的系统字体列表（首次扫描后永久缓存）。
///
/// 字体列表在应用运行期间不会变化，因此可安全全局共享。
static CACHED_FONTS: OnceLock<Vec<FontInfo>> = OnceLock::new();

/// 获取缓存的系统字体列表。
///
/// 首次调用时触发真实扫描（约 1s），后续直接返回缓存引用。
/// 配合启动时 `prewarm_font_cache()` 可消除对话框初始化时的扫码延迟。
pub fn get_cached_fonts() -> &'static [FontInfo] {
    CACHED_FONTS.get_or_init(|| {
        puffin::profile_scope!("scan_system_fonts_cached");
        let fonts = scan_system_fonts();
        tracing::info!("系统字体扫描完成，共 {} 个字体", fonts.len());
        fonts
    })
}

/// 在后台线程预热字体缓存。
///
/// 应在应用启动的早期调用（例如主窗口创建后），
/// 使首次打开设置对话框时字体列表已缓存，不会阻塞 UI 线程。
///
/// 与 `prewarm_dialog_shared_engine` 同样的后台预热 pattern：
/// 用 `std::thread::spawn` 将耗时的系统字体枚举移到后台，
/// 主线程渲染时调用 `get_cached_fonts()` 直接获取已缓存的结果。
pub fn prewarm_font_cache() {
    std::thread::spawn(|| {
        puffin::profile_scope!("prewarm_font_cache");
        let _ = get_cached_fonts();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_system_fonts() {
        let fonts = scan_system_fonts();
        // 只检查是否能运行，不同系统字体数量不同
        println!("Found {} fonts", fonts.len());
        for font in fonts.iter().take(10) {
            println!("  - {} ({:?})", font.name, font.path);
        }
        // 确保返回的字体都有名称和路径
        for font in &fonts {
            assert!(!font.name.is_empty(), "Font name should not be empty");
            assert!(
                font.path.exists(),
                "Font path should exist: {:?}",
                font.path
            );
        }
    }

    #[test]
    fn test_font_info_display() {
        let font = FontInfo {
            name: "Arial".to_string(),
            path: PathBuf::from("/path/to/arial.ttf"),
        };
        assert_eq!(format!("{}", font), "Arial");
    }

    #[test]
    fn test_font_info_sorting() {
        let mut fonts = [
            FontInfo {
                name: "Zebra".to_string(),
                path: PathBuf::from("/path/to/zebra.ttf"),
            },
            FontInfo {
                name: "Arial".to_string(),
                path: PathBuf::from("/path/to/arial.ttf"),
            },
            FontInfo {
                name: "bold".to_string(),
                path: PathBuf::from("/path/to/bold.ttf"),
            },
        ];
        fonts.sort_by_key(|a| a.name.to_lowercase());
        assert_eq!(fonts[0].name, "Arial");
        assert_eq!(fonts[1].name, "bold");
        assert_eq!(fonts[2].name, "Zebra");
    }
}
