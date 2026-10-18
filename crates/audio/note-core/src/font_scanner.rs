//! 跨平台字体扫描模块
//!
//! 实现改用 `fontdb`（枚举系统字体）+ `ab_glyph`（字形覆盖检测），**不再依赖
//! font-kit**。原因：
//! - font-kit 在 Android 上会强编 `yeslogic-fontconfig-sys` / `freetype-sys`，
//!   而 NDK 里没有 `fontconfig.pc`，构建脚本直接失败（连 crate 都编不过）；
//! - `fontdb` 是 cosmic-text 的既有依赖（同版本 0.23），零新增编译成本，且全平台可用。
//!
//! Android 特别注意：`fontdb` 的 `load_system_fonts()` 在 android 上被显式跳过
//! （`cfg` 排除），所以必须自己 `load_fonts_dir("/system/fonts")`，否则字体列表为空。
//!
//! 字体扫描是重操作（Windows 上枚举 200-500+ 字体），
//! 通过全局 OnceLock 缓存避免每次对话框重建时重复扫描约 1.3s 的延迟。

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// 字体信息
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontInfo {
    /// 字体名称（从字体文件的 name 表解析出的真实家族名）
    pub name: String,
    /// 字体文件路径
    pub path: PathBuf,
}

impl std::fmt::Display for FontInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

/// Android 系统字体目录（`fontdb` 不会自动扫描这些目录）
#[cfg(target_os = "android")]
const ANDROID_FONT_DIRS: &[&str] = &["/system/fonts", "/product/fonts", "/system_ext/fonts"];

/// 构造已加载系统字体的字体库
fn system_database() -> fontdb::Database {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();

    #[cfg(target_os = "android")]
    for dir in ANDROID_FONT_DIRS {
        db.load_fonts_dir(dir);
    }

    db
}

/// 扫描系统字体，返回可用字体列表
///
/// 每个家族取第一个字面（face）；同一路径只会出现一次（字体集合内多字面共用路径）。
pub fn scan_system_fonts() -> Vec<FontInfo> {
    let db = system_database();

    let mut fonts: Vec<FontInfo> = db
        .faces()
        .filter_map(|face| {
            // 家族名优先取 name 表里的第一个（通常为英文名）
            let name = face.families.first()?.0.clone();

            let path = match &face.source {
                fontdb::Source::File(path) => path.clone(),
                fontdb::Source::SharedFile(path, _) => path.clone(),
                // 内存字体（无实际文件路径）不参与「按路径加载」的调用方
                fontdb::Source::Binary(_) => return None,
            };

            if name.is_empty() || !path.is_file() {
                return None;
            }

            Some(FontInfo { name, path })
        })
        .collect();

    // 按名称排序（不区分大小写），并去重同名字体
    fonts.sort_by_key(|font| font.name.to_lowercase());
    fonts.dedup_by(|a, b| a.path == b.path);

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
pub fn prewarm_font_cache() {
    std::thread::spawn(|| {
        puffin::profile_scope!("prewarm_font_cache");
        let _ = get_cached_fonts();
    });
}

/// 解析字体文件的家族名（family name）
///
/// 用于「自定义字体文件路径」场景：文件注册进字体库后，需按家族名引用。
/// 解析失败（文件不存在/损坏/非字体）返回 `None`。
pub fn resolve_font_family(path: &Path) -> Option<String> {
    let mut db = fontdb::Database::new();
    db.load_font_file(path).ok()?;

    let name = db.faces().next()?.families.first()?.0.clone();
    (!name.is_empty()).then_some(name)
}

/// 检测字体（按名称或文件路径）是否覆盖 CJK 字形
///
/// - `name_or_path` 是存在的文件路径 → 直接检测该文件；
/// - 否则按系统字体名（不区分大小写）从缓存中查找；
/// - 用代表性汉字探测（同一字体家族内覆盖一致），任一命中即视为覆盖。
pub fn font_supports_cjk(name_or_path: &str) -> bool {
    let Some(path) = resolve_font_path(name_or_path) else {
        return false;
    };

    let Ok(bytes) = std::fs::read(&path) else {
        return false;
    };

    // 字体集合（.ttc）取第 0 个字面
    let Ok(font) = ab_glyph::FontVec::try_from_vec_and_index(bytes, 0) else {
        return false;
    };

    use ab_glyph::Font as _;

    const PROBE_CHARS: [char; 4] = ['中', '文', '测', '试'];
    PROBE_CHARS.iter().any(|&probe| font.glyph_id(probe).0 != 0)
}

/// 从系统字体链中挑一个覆盖 CJK 的字体（不写死单一字体）
///
/// 先按跨平台常见 CJK 家族名候选匹配（Windows/macOS/Linux/Android 各平台候选），
/// 再用字形覆盖校验；候选均不命中时，按名称关键字兜底扫描（有上限），
/// 仍无命中则返回 `None`（调用方回退默认字体；渲染层另有 cosmic-text 自动回退兜底）。
pub fn pick_cjk_font() -> Option<&'static FontInfo> {
    /// 常见 CJK 字体家族候选（按平台习惯排序；末尾三条为 Android 系统字体）
    const CANDIDATES: [&str; 15] = [
        "Microsoft YaHei UI",
        "Microsoft YaHei",
        "SimSun",
        "PingFang SC",
        "Hiragino Sans GB",
        "Noto Sans CJK SC",
        "Source Han Sans SC",
        "WenQuanYi Zen Hei",
        "WenQuanYi Micro Hei",
        "Malgun Gothic",
        "Yu Gothic",
        "Meiryo",
        "Noto Sans SC",
        "Noto Sans CJK",
        "Droid Sans Fallback",
    ];
    /// 名称关键字兜底扫描的最大检测数量（避免全量加载系统字体）
    const SCAN_LIMIT: usize = 40;

    let fonts = get_cached_fonts();

    // 1) 候选家族名精确匹配（不区分大小写）+ 覆盖校验
    for candidate in CANDIDATES {
        if let Some(font) = fonts
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(candidate))
            && font_supports_cjk(&font.path.to_string_lossy())
        {
            return Some(font);
        }
    }

    // 2) 名称关键字兜底（如 "XXX Hei/Song/Ming/Kai/CJK"）
    const KEYWORDS: [&str; 6] = ["CJK", "Hei", "Song", "Ming", "Kai", "Gothic"];
    fonts
        .iter()
        .filter(|f| KEYWORDS.iter().any(|k| f.name.contains(k)))
        .take(SCAN_LIMIT)
        .find(|f| font_supports_cjk(&f.path.to_string_lossy()))
}

/// 解析「名称或路径」到实际字体文件路径
fn resolve_font_path(name_or_path: &str) -> Option<PathBuf> {
    let path = Path::new(name_or_path);
    if path.is_file() {
        return Some(path.to_path_buf());
    }
    get_cached_fonts()
        .iter()
        .find(|f| f.name.eq_ignore_ascii_case(name_or_path))
        .map(|f| f.path.clone())
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

    #[test]
    fn test_resolve_font_family_from_system_font() {
        // 从系统字体缓存取第一个字体文件，验证 family 解析链路可用
        let Some(first) = get_cached_fonts().first() else {
            println!("系统无可用字体，跳过");
            return;
        };
        let family = resolve_font_family(&first.path);
        assert!(
            family.as_deref().is_some_and(|f| !f.is_empty()),
            "字体 family 解析失败: {:?}",
            first.path
        );
    }

    #[test]
    fn test_resolve_font_family_invalid_path() {
        assert!(resolve_font_family(Path::new("C:/definitely/not/exist.ttf")).is_none());
    }

    #[test]
    fn test_font_supports_cjk_known_fonts() {
        // 仅对系统实际存在的字体断言（跨平台容错）
        let fonts = get_cached_fonts();
        for name in [
            "Microsoft YaHei",
            "SimSun",
            "PingFang SC",
            "Noto Sans CJK SC",
            "Noto Sans SC",
            "WenQuanYi Zen Hei",
        ] {
            if fonts.iter().any(|f| f.name.eq_ignore_ascii_case(name)) {
                assert!(font_supports_cjk(name), "应覆盖 CJK: {name}");
            }
        }
        for name in ["Arial", "DejaVu Sans", "Liberation Sans"] {
            if fonts.iter().any(|f| f.name.eq_ignore_ascii_case(name)) {
                assert!(!font_supports_cjk(name), "不应覆盖 CJK: {name}");
            }
        }
    }

    #[test]
    fn test_font_supports_cjk_unknown_name() {
        assert!(!font_supports_cjk("NoSuchFontXYZ"));
    }

    #[test]
    fn test_pick_cjk_font_coverage() {
        match pick_cjk_font() {
            Some(font) => assert!(
                font_supports_cjk(&font.path.to_string_lossy()),
                "挑出的字体必须覆盖 CJK: {}",
                font.name
            ),
            None => println!("系统未找到 CJK 字体（跳过）"),
        }
    }
}
