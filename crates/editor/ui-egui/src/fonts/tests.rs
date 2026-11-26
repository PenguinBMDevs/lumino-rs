//! `fonts` 单元测试（REF-001：保持 `fonts.rs` ≤600 行）。

use super::*;

/// 串行化会修改全局字体策略的用例（`cargo test` 同进程并行时也安全；
/// `cargo nextest` 本就一进程一用例）。
static FONT_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn font_test_lock() -> std::sync::MutexGuard<'static, ()> {
    FONT_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

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
fn test_symbol_fallback_covers_ui_icons() {
    let ctx = egui::Context::default();
    install(&ctx);

    // 这些符号会直接出现在 UI 文案里（返回箭头 / 菜单箭头 / 选中标记）；
    // 比例字体链必须能解析它们，否则渲染成豆腐块。
    let font = egui::FontId::proportional(12.0);
    // `set_fonts` 在首帧 `run_ui` 时才真正建字体表，检查必须放在帧内。
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        for ch in ['←', '→', '●', '▾', '📁', '📄'] {
            assert!(
                ui.ctx().fonts_mut(|fonts| fonts.has_glyph(&font, ch)),
                "比例字体链缺少符号字形：{ch:?}"
            );
        }
    });
    // 测试不接管渲染后端，纹理增量主动丢弃，避免 `Drop` 断言。
    output.textures_delta.clear();
}

#[test]
fn test_bundled_font_always_available() {
    let _guard = font_test_lock();
    // 该用例断言「未配置程序字体」的默认链；先归一化全局状态。
    let _ = configure(None);
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
    // 未配置程序字体时不得污染字体链（视觉基线依赖内置字体顺序不变）。
    assert!(
        !definitions.font_data.contains_key(PROGRAM_FONT_KEY),
        "未配置程序字体时不得注入程序字体键"
    );
    assert!(
        definitions
            .families
            .get(&FontFamily::Proportional)
            .is_some_and(|keys| !keys.iter().any(|k| k == PROGRAM_FONT_KEY)),
        "未配置程序字体时比例字体链不得包含程序字体"
    );
    assert_eq!(resolved_name(), Some(FONT_NAME));
}

fn sample_program_font() -> ProgramFont {
    ProgramFont {
        source: "test://bundled".to_owned(),
        family: FONT_NAME.to_owned(),
        bytes: Arc::new(BUNDLED_CJK.to_vec()),
        face_index: 0,
    }
}

#[test]
fn test_program_font_prepended_to_proportional_only() {
    let _guard = font_test_lock();
    let _ = configure(None);
    assert!(configure(Some(sample_program_font())));

    let definitions = definitions().expect("内置字体恒可用");
    assert!(
        definitions.font_data.contains_key(PROGRAM_FONT_KEY),
        "程序字体必须写入 font_data"
    );
    let proportional = definitions
        .families
        .get(&FontFamily::Proportional)
        .expect("必须存在比例字体链");
    assert_eq!(
        proportional.first().map(String::as_str),
        Some(PROGRAM_FONT_KEY),
        "程序字体必须位于比例字体链首位"
    );
    assert!(
        proportional.iter().any(|key| key == FONT_KEY),
        "内置 CJK 必须继续留在链尾做回退"
    );
    let monospace = definitions
        .families
        .get(&FontFamily::Monospace)
        .expect("必须存在等宽字体链");
    assert_ne!(
        monospace.first().map(String::as_str),
        Some(PROGRAM_FONT_KEY),
        "程序字体不得破坏等宽语义"
    );

    let _ = configure(None);
}

#[test]
fn test_configure_same_source_is_noop() {
    let _guard = font_test_lock();
    let _ = configure(None);
    assert!(configure(Some(sample_program_font())), "首次配置必须生效");
    assert!(
        !configure(Some(sample_program_font())),
        "同一来源重复配置不得触发字体重建"
    );
    assert!(configure(None), "清空配置必须生效");
    let _ = configure(None);
}

#[test]
fn test_install_rebuilds_after_generation_bump() {
    let _guard = font_test_lock();
    let _ = configure(None);
    let ctx = egui::Context::default();
    install(&ctx);
    let marker = marker_id();
    let first = ctx
        .data(|data| data.get_temp::<u64>(marker))
        .expect("首次 install 必须写入代数标记");

    assert!(configure(Some(sample_program_font())));
    install(&ctx);
    let second = ctx
        .data(|data| data.get_temp::<u64>(marker))
        .expect("二次 install 必须更新代数标记");
    assert_ne!(first, second, "程序字体变化后必须重建字体表");

    let _ = configure(None);
}

/// 在真实 egui 排版中量一段文本的宽度（首帧建表）。
fn layout_width(ctx: &egui::Context, text: &str) -> f32 {
    let mut width = 0.0_f32;
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        width = ui.ctx().fonts_mut(|fonts| {
            fonts
                .layout_no_wrap(
                    text.to_owned(),
                    egui::FontId::proportional(16.0),
                    egui::Color32::WHITE,
                )
                .size()
                .x
        });
    });
    output.textures_delta.clear();
    width
}

#[test]
fn test_program_font_takes_precedence_in_layout() {
    let _guard = font_test_lock();

    // 未配置：拉丁字符走 egui 默认 `Ubuntu-Light`。
    let _ = configure(None);
    let ctx = egui::Context::default();
    install(&ctx);
    let before = layout_width(&ctx, "Program Font 123 ABC");

    // 配置内置 Noto Sans SC 为程序字体：拉丁字符必须改由它最先提供，
    // 字宽随字体变化 = 字体真的参与了排版，而不只是写进了 FontDefinitions。
    assert!(configure(Some(sample_program_font())));
    install(&ctx);
    let after = layout_width(&ctx, "Program Font 123 ABC");
    assert_ne!(
        before, after,
        "配置程序字体后排版宽度必须变化（{before} -> {after}）"
    );

    let _ = configure(None);
}
