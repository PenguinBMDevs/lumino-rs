//! 计数器 TTF 字体加载（系统字体路径表 + 字节解析）
//!
//! 与渲染逻辑分离（`counter_font_ttf.rs` 只负责光栅化与缓存），控制文件行数。

use ab_glyph::{FontArc, FontVec};
use lumino_message::events::window::video::CounterFont;

/// 系统字体路径表（按名称查找）。
///
/// 优先 Windows 常见中文字体；macOS/Linux 提供常见路径作为兜底。
pub(super) fn system_font_path(family: &str) -> Option<std::path::PathBuf> {
    let mut candidates: Vec<String> = Vec::new();
    #[cfg(target_os = "windows")]
    {
        let dir = "C:\\Windows\\Fonts\\";
        candidates.extend(match family {
            "微软雅黑" => vec![format!("{dir}msyh.ttc")],
            "微软雅黑粗体" => vec![format!("{dir}msyhbd.ttc")],
            "宋体" => vec![format!("{dir}simsun.ttc")],
            "黑体" => vec![format!("{dir}simhei.ttf")],
            "楷体" => vec![format!("{dir}simkai.ttf")],
            "仿宋" => vec![format!("{dir}simfang.ttf")],
            "Arial" => vec![format!("{dir}arial.ttf")],
            "Consolas" => vec![format!("{dir}consola.ttf")],
            _ => Vec::new(),
        });
    }
    #[cfg(not(target_os = "windows"))]
    {
        candidates.extend(match family {
            "微软雅黑" | "宋体" | "黑体" => vec![
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc".to_string(),
                "/System/Library/Fonts/PingFang.ttc".to_string(),
            ],
            "Arial" => vec![
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf".to_string(),
                "/System/Library/Fonts/Supplemental/Arial.ttf".to_string(),
            ],
            "Consolas" => vec![
                "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf".to_string(),
                "/System/Library/Fonts/Supplemental/Courier New.ttf".to_string(),
            ],
            _ => Vec::new(),
        });
    }
    candidates
        .iter()
        .map(std::path::PathBuf::from)
        .find(|p| p.is_file())
}

/// 从字节加载字体（兼容单字体文件与 TTC 集合）。
pub(super) fn load_font_bytes(bytes: Vec<u8>) -> Result<FontArc, String> {
    // 先按单字体文件解析；TTC 集合（微软雅黑/宋体）解析失败时取第 0 个 face。
    if let Ok(font) = FontVec::try_from_vec(bytes.clone()) {
        return Ok(font.into());
    }
    FontVec::try_from_vec_and_index(bytes, 0)
        .map(FontArc::from)
        .map_err(|e| format!("字体文件解析失败: {e}"))
}

/// 按来源加载字体。
pub(super) fn load_font(font: &CounterFont) -> Result<FontArc, String> {
    match font {
        CounterFont::Bitmap => Err("内置点阵字体不需要 TTF 后端".to_string()),
        CounterFont::System { family } => {
            let path =
                system_font_path(family).ok_or_else(|| format!("未找到系统字体「{family}」"))?;
            load_font_bytes(
                std::fs::read(&path)
                    .map_err(|e| format!("读取字体文件 {} 失败: {e}", path.display()))?,
            )
        }
        CounterFont::File { path } => {
            if path.is_empty() {
                return Err("未指定自定义字体文件路径".to_string());
            }
            load_font_bytes(
                std::fs::read(path).map_err(|e| format!("读取字体文件 {path} 失败: {e}"))?,
            )
        }
    }
}
