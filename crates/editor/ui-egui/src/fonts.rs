//! 确定性 CJK 字体注入。
//!
//! egui 0.37 不再默认携带完整 CJK 字体，而系统字体因平台而异：
//! Windows 有微软雅黑、macOS 有苹方、CI 的精简 Ubuntu 可能一个 CJK 字体都没有。
//! 这会让中文渲染逐像素不同，进而使跨平台视觉基线测试（bridge_parity /
//! bands_pixels）无法复现。
//!
//! 因此这里统一使用仓库内置的 Noto Sans SC（GB2312 子集），通过 `include_bytes!`
//! 编译期内嵌。所有平台、所有环境（开发机与 CI）使用同一份字体字节，
//! 中文渲染确定性一致——基线可在任意平台重新捕获并复现。

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};

/// 仓库内置 CJK 字体（GB2312 子集，约 2.2 MB），随二进制分发、无外部文件依赖。
///
/// 路径相对 `CARGO_MANIFEST_DIR`（`crates/editor/ui-egui`），上溯 3 级到工作区根。
const BUNDLED_CJK: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../resources/fonts/NotoSansSC-Regular.ttf"
));

/// 注入字体的家族键（proportional / monospace fallback 链末尾）。
const FONT_KEY: &str = "lumino-cjk";

/// 字体族名（诊断 / 文档用）。
const FONT_NAME: &str = "Noto Sans SC (bundled)";

/// 在给定 Context 上安装 CJK 字体，并确保缺字形时不会 panic。
///
/// `egui_kittest` 默认把 `MissingGlyphPolicy` 设为 `Panic`（用于捕捉测试里意外
/// 缺失的字形）。内置字体虽覆盖常用中文，但遇到生僻字仍可能缺字形；显式设为
/// `Tofu` 可让无头测试在任意环境都不因字形缺失而崩溃。
///
/// 生产环境 epaint 默认本就是 `Tofu`，故此改动不改变生产渲染行为。该调用幂等，
/// 每帧重复调用无副作用。
pub fn install(ctx: &egui::Context) {
    ctx.set_missing_glyph_policy(egui::MissingGlyphPolicy::Tofu);

    let marker = egui::Id::unique("lumino-ui-egui-fonts-installed");
    if ctx.data(|data| data.get_temp::<bool>(marker).unwrap_or(false)) {
        return;
    }

    if let Some(definitions) = definitions() {
        ctx.set_fonts(definitions);
    }
    ctx.data_mut(|data| data.insert_temp(marker, true));
}

/// 构造带内置 CJK fallback 的 `FontDefinitions`。
///
/// 内置字体编译期内嵌，恒可用，因此始终返回 `Some`（保留 `Option` 签名是为
/// 兼容既有调用方）。
pub fn definitions() -> Option<FontDefinitions> {
    let mut definitions = FontDefinitions::default();
    definitions.font_data.insert(
        FONT_KEY.to_owned(),
        FontData {
            font: Arc::new(BUNDLED_CJK.to_vec()),
            index: 0,
            tweak: Default::default(),
        }
        .into(),
    );

    // 默认拉丁字体在前，CJK 作为 fallback 追加在末尾。
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        definitions
            .families
            .entry(family)
            .or_default()
            .push(FONT_KEY.to_owned());
    }

    tracing::info!(
        "egui: 注入内置 CJK 字体 '{}'（{} bytes）",
        FONT_NAME,
        BUNDLED_CJK.len()
    );
    Some(definitions)
}

/// 已解析字体族名（用于诊断 / 文档）。
pub fn resolved_name() -> Option<&'static str> {
    Some(FONT_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_allows_missing_glyphs_without_panic() {
        let ctx = egui::Context::default();
        // 模拟 egui_kittest 的默认行为：缺字形直接 panic。
        ctx.set_missing_glyph_policy(egui::MissingGlyphPolicy::Panic);
        assert_eq!(
            ctx.missing_glyph_policy(),
            egui::MissingGlyphPolicy::Panic,
            "前置条件：默认应为 Panic"
        );

        install(&ctx);

        assert_eq!(
            ctx.missing_glyph_policy(),
            egui::MissingGlyphPolicy::Tofu,
            "install 后缺字形策略必须为 Tofu"
        );
    }

    #[test]
    fn test_bundled_font_always_available() {
        // 内置字体编译期内嵌，任何平台都不依赖系统字体。
        assert!(!BUNDLED_CJK.is_empty(), "内置 CJK 字体字节不能为空");
        let definitions = definitions().expect("内置字体恒可用，definitions 必为 Some");
        assert!(
            definitions.font_data.contains_key(FONT_KEY),
            "字体定义必须包含内置 CJK 键"
        );
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            assert!(
                definitions
                    .families
                    .get(&family)
                    .is_some_and(|keys| keys.iter().any(|k| k == FONT_KEY)),
                "{family:?} fallback 链必须包含内置 CJK 字体"
            );
        }
        assert_eq!(resolved_name(), Some(FONT_NAME));
    }
}
