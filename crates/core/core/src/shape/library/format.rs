//! `.lmshape` 库文件读写（DRAW-033A）
//!
//! 文件是 UTF-8 JSON：`magic + version + ShapeDrawing`。选 JSON 而不是 bincode
//! 的原因是库文件需要“未知字段忽略 + 坏文件定位”：serde 默认忽略未知字段，
//! 而 bincode 的位置编码无法做到。写入仍走共享的原子替换工具。
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::io::atomic::write_file_atomic;

use super::LibraryWarning;
use super::drawing::ShapeDrawing;

/// 库文件魔数
pub const MAGIC: &str = "LMSD";
/// 库文件版本（只接受 1；未来版本跳过并 warning）
pub const VERSION: u16 = 1;
/// 单文件最大字节数（防损坏文件/压缩炸弹式输入）
pub const MAX_BYTES: usize = 4 * 1024 * 1024;
/// 扩展名
pub const EXTENSION: &str = "lmshape";

#[derive(Debug, Serialize, Deserialize)]
struct ShapeFile {
    magic: String,
    version: u16,
    #[serde(flatten)]
    drawing: ShapeDrawing,
}

/// 把图形编码为文件字节（校验失败不写盘）
pub fn encode_drawing(drawing: &ShapeDrawing) -> Result<Vec<u8>> {
    drawing.validate()?;
    let file = ShapeFile {
        magic: MAGIC.to_string(),
        version: VERSION,
        drawing: drawing.clone(),
    };
    let bytes = serde_json::to_vec_pretty(&file)?;
    if bytes.len() > MAX_BYTES {
        return Err(CoreError::FileFormat(format!(
            "形状文件超过上限（最多 {MAX_BYTES} 字节，实际 {}）",
            bytes.len()
        )));
    }
    Ok(bytes)
}

/// 从字节解码；损坏/未来版本返回可读错误
pub fn decode_drawing(bytes: &[u8]) -> Result<ShapeDrawing> {
    if bytes.len() > MAX_BYTES {
        return Err(CoreError::FileFormat(format!(
            "形状文件超过上限（最多 {MAX_BYTES} 字节）"
        )));
    }
    let file: ShapeFile = serde_json::from_slice(bytes)?;
    if file.magic != MAGIC {
        return Err(CoreError::FileFormat(format!(
            "形状文件魔数不匹配（期望 {MAGIC}，实际 {}）",
            file.magic
        )));
    }
    if file.version != VERSION {
        return Err(CoreError::FileFormat(format!(
            "不支持的形状文件版本 {}（当前 {VERSION}）",
            file.version
        )));
    }
    file.drawing.validate()?;
    Ok(file.drawing)
}

/// 扫描用户库目录；坏文件跳过、其余继续加载。
pub(crate) fn load_user_dir(dir: &Path) -> (Vec<ShapeDrawing>, Vec<LibraryWarning>) {
    let mut drawings = Vec::new();
    let mut warnings = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return (drawings, warnings);
        }
        Err(error) => {
            warnings.push(LibraryWarning {
                path: dir.to_path_buf(),
                message: format!("无法读取形状库目录：{error}"),
            });
            return (drawings, warnings);
        }
    };

    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case(EXTENSION))
        })
        .collect();
    paths.sort();

    for path in paths {
        if let Ok(metadata) = std::fs::metadata(&path)
            && metadata.is_file()
            && metadata.len() > MAX_BYTES as u64
        {
            warnings.push(LibraryWarning {
                path,
                message: format!("形状文件超过上限（最多 {MAX_BYTES} 字节）"),
            });
            continue;
        }
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                warnings.push(LibraryWarning {
                    path,
                    message: format!("读取失败：{error}"),
                });
                continue;
            }
        };
        let mut drawing = match decode_drawing(&bytes) {
            Ok(drawing) => drawing,
            Err(error) => {
                warnings.push(LibraryWarning {
                    path,
                    message: error.to_string(),
                });
                continue;
            }
        };
        if drawing.id.trim().is_empty() {
            drawing.id = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| "shape".to_string());
        }
        let unique = unique_id(&drawings, &drawing.id);
        if unique != drawing.id {
            warnings.push(LibraryWarning {
                path: path.clone(),
                message: format!("形状 id 重复，已重命名为 {}", unique),
            });
            drawing.id = unique;
        }
        drawings.push(drawing);
    }
    (drawings, warnings)
}

/// 原子保存单个图形；目标文件由 `drawing.id` 决定。
pub fn save_drawing(dir: &Path, drawing: &ShapeDrawing) -> Result<()> {
    drawing.validate()?;
    std::fs::create_dir_all(dir)?;
    let path = drawing_path(dir, &drawing.id);
    let bytes = encode_drawing(drawing)?;
    write_file_atomic(&path, &bytes)?;
    Ok(())
}

/// 删除单个图形；文件不存在返回 `Ok(false)`。
pub fn delete_drawing(dir: &Path, id: &str) -> Result<bool> {
    let path = drawing_path(dir, id);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// 图形文件路径（id 会被安全化，禁止路径穿越）
pub fn drawing_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{}.{}", safe_file_stem(id), EXTENSION))
}

fn unique_id(existing: &[ShapeDrawing], id: &str) -> String {
    if !existing.iter().any(|drawing| drawing.id == id) {
        return id.to_string();
    }
    for suffix in 2..=u32::MAX {
        let candidate = format!("{id}-{suffix}");
        if !existing.iter().any(|drawing| drawing.id == candidate) {
            return candidate;
        }
    }
    format!("{id}-dup")
}

/// 把任意 id 变成安全文件名：保留字母/数字（含 CJK）、`-`、`_`，其余替换为 `_`。
pub(crate) fn safe_file_stem(id: &str) -> String {
    let mut out = String::new();
    for ch in id.chars().take(64) {
        if ch.is_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let out = out.trim_matches('_').trim().to_string();
    let out = if out.is_empty() {
        "shape".to_string()
    } else {
        out
    };
    if is_windows_reserved(&out) {
        format!("shape-{out}")
    } else {
        out
    }
}

fn is_windows_reserved(name: &str) -> bool {
    matches!(
        name.to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::library::builtin::{BuiltinShape, builtin_drawings};

    #[test]
    fn test_json_roundtrip_preserves_all_fields() {
        let drawing = builtin_drawings()
            .into_iter()
            .find(|drawing| drawing.id == BuiltinShape::Circle.id())
            .expect("内置圆");
        let bytes = encode_drawing(&drawing).expect("编码失败");
        let text = String::from_utf8(bytes.clone()).expect("JSON 应为 UTF-8");
        assert!(text.contains("\"magic\": \"LMSD\""));
        assert!(text.contains("\"version\": 1"));
        let back = decode_drawing(&bytes).expect("解码失败");
        assert_eq!(back, drawing);
    }

    #[test]
    fn test_decode_ignores_unknown_fields() {
        let drawing = BuiltinShape::Square.drawing();
        let mut value = serde_json::to_value(ShapeFile {
            magic: MAGIC.to_string(),
            version: VERSION,
            drawing,
        })
        .expect("to_value 失败");
        value
            .as_object_mut()
            .expect("对象")
            .insert("future_field".to_string(), serde_json::json!(42));
        let bytes = serde_json::to_vec(&value).expect("序列化失败");
        decode_drawing(&bytes).expect("未知字段必须被忽略");
    }

    #[test]
    fn test_decode_rejects_bad_magic_and_future_version() {
        let drawing = BuiltinShape::Triangle.drawing();
        let mut value = serde_json::to_value(ShapeFile {
            magic: "NOPE".to_string(),
            version: VERSION,
            drawing: drawing.clone(),
        })
        .expect("to_value 失败");
        let bytes = serde_json::to_vec(&value).expect("序列化失败");
        assert!(decode_drawing(&bytes).is_err(), "坏魔数必须被拒绝");

        value
            .as_object_mut()
            .expect("对象")
            .insert("magic".to_string(), serde_json::json!(MAGIC));
        value
            .as_object_mut()
            .expect("对象")
            .insert("version".to_string(), serde_json::json!(VERSION + 1));
        let bytes = serde_json::to_vec(&value).expect("序列化失败");
        assert!(decode_drawing(&bytes).is_err(), "未来版本必须被拒绝");
    }

    #[test]
    fn test_safe_file_stem_blocks_traversal_and_reserved_names() {
        assert_eq!(safe_file_stem("../a/b"), "a_b");
        assert_eq!(safe_file_stem("CON"), "shape-CON");
        assert_eq!(safe_file_stem("  "), "shape");
        assert_eq!(safe_file_stem("圆形-1"), "圆形-1");
    }
}
