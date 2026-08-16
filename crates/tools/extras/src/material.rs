//! 编译时嵌入的素材（.lmmaterial）数据
//!
//! 从 `resources/Materials/` 目录自动检测并嵌入：
//! - `EmbeddedMaterial` 仅包含文件名（显示名）与原始字节；
//! - 素材内容（音轨数/多轨标记/音符数据）在运行时由调用方
//!   （如 lumino-ui 右侧栏）通过 `lumino_export` 解析，本 crate
//!   保持零依赖（仅 lumino-core），不做格式解析。

/// 编译时嵌入的素材数据
#[derive(Debug, Clone, Copy)]
pub struct EmbeddedMaterial {
    /// 显示名称（文件名不含扩展名）
    pub name: &'static str,
    /// .lmmaterial 归档原始字节
    pub data: &'static [u8],
}

// ─── 编译时嵌入的数据 ─────────────────────────────────────────────────────────
// 由 build.rs 自动生成，扫描 `resources/Materials/` 目录下的所有 .lmmaterial 文件

/// 所有嵌入的素材原始数据（编译时自动检测）
///
/// 由 build.rs 在编译时根据 resources/Materials/ 目录生成
pub fn embedded_materials() -> &'static [EmbeddedMaterial] {
    const DATA: &[EmbeddedMaterial] = &include!(concat!(env!("OUT_DIR"), "/materials.rs"));
    DATA
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_materials_list_static() {
        // 嵌入列表必须可静态访问（编译期生成，运行时不依赖文件系统）
        let materials = embedded_materials();
        for m in materials {
            assert!(!m.name.is_empty());
            // 归档字节必须非空（LMPJ 魔数至少 4 字节）
            assert!(m.data.len() >= 4, "素材 {} 字节为空", m.name);
        }
    }
}
