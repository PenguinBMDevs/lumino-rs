//! 调色板管理器（全局单例）

use std::sync::LazyLock;

use super::png::decode_palette_png;
use super::{EmbeddedPalette, FALLBACK_PALETTE, Palette};

/// 所有嵌入的调色板原始数据（编译时自动检测）
///
/// 由 build.rs 在编译时根据 resources/palettes/ 目录生成
fn embedded_palettes() -> &'static [EmbeddedPalette] {
    const DATA: &[EmbeddedPalette] = &include!(concat!(env!("OUT_DIR"), "/palettes.rs"));
    DATA
}

/// 调色板管理器（全局单例）
///
/// 管理所有编译时嵌入的调色板，提供查询和获取功能。
pub static PALETTE_MANAGER: LazyLock<PaletteManager> = LazyLock::new(PaletteManager::new);

/// 调色板管理器
#[derive(Debug)]
pub struct PaletteManager {
    /// 所有已解析的调色板
    palettes: Vec<Palette>,
    /// 调色板名称列表
    names: Vec<&'static str>,
}

impl PaletteManager {
    /// 创建新的调色板管理器（解析所有嵌入数据）
    fn new() -> Self {
        let mut palettes: Vec<Palette> = embedded_palettes()
            .iter()
            .filter_map(|ep| match decode_palette_png(ep.data) {
                Ok(colors) => Some(Palette {
                    name: ep.name,
                    colors,
                }),
                Err(e) => {
                    tracing::warn!("[Palette] 解析调色板失败 '{}': {}", ep.name, e);
                    None
                }
            })
            .collect();

        // 如果没有成功解析任何调色板，使用硬编码备用
        if palettes.is_empty() {
            tracing::warn!("[Palette] 没有可用的调色板文件，使用硬编码备用");
            palettes.push(Palette {
                name: "Default",
                colors: FALLBACK_PALETTE.to_vec(),
            });
        }

        // 将名为 "Random" 的调色板冒泡到第一个位置，作为默认调色板
        if let Some(random_idx) = palettes.iter().position(|palette| palette.name == "Random")
            && random_idx != 0
        {
            let random = palettes.remove(random_idx);
            palettes.insert(0, random);
        }

        let names = palettes.iter().map(|palette| palette.name).collect();
        Self { palettes, names }
    }

    /// 获取所有调色板名称
    pub fn names(&self) -> &[&'static str] {
        &self.names
    }

    /// 获取所有调色板（引用）
    pub fn palettes(&self) -> &[Palette] {
        &self.palettes
    }

    /// 按名称获取调色板
    pub fn get(&self, name: &str) -> Option<&Palette> {
        self.palettes.iter().find(|palette| palette.name == name)
    }

    /// 获取默认调色板（第一个）
    pub fn default(&self) -> &Palette {
        &self.palettes[0]
    }

    /// 获取有效调色板名称（如果不存在则返回默认名称）
    pub fn resolve_name(&self, name: &str) -> &'static str {
        for &name_str in &self.names {
            if name_str == name {
                return name_str;
            }
        }
        self.names[0]
    }
}
