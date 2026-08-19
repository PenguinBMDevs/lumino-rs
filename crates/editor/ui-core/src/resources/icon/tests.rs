//! 图标系统测试：SVG 渲染、主题反色、缓存与全量回归

use super::*;
#[test]
fn test_invert_rgba_inverts_opaque_pixels() {
    let rgba = vec![10, 20, 30, 255, 40, 50, 60, 255];
    let inverted = invert_rgba(&rgba);
    assert_eq!(inverted, vec![245, 235, 225, 255, 215, 205, 195, 255]);
}

#[test]
fn test_invert_rgba_preserves_transparent_pixels() {
    let rgba = vec![10, 20, 30, 0, 40, 50, 60, 128];
    let inverted = invert_rgba(&rgba);
    // 完全透明像素保持原样，半透明像素仍做反色
    assert_eq!(inverted, vec![10, 20, 30, 0, 215, 205, 195, 128]);
}

#[test]
fn test_logo_icons_never_invert() {
    assert!(!should_invert_icon(Icon::Lumino, true));
    assert!(!should_invert_icon(Icon::LogoInApp, true));
    assert!(!should_invert_icon(Icon::Lumino, false));
    assert!(!should_invert_icon(Icon::LogoInApp, false));
}

#[test]
fn test_regular_icons_invert_only_in_dark_mode() {
    assert!(should_invert_icon(Icon::Gear, true));
    assert!(!should_invert_icon(Icon::Gear, false));
    assert!(should_invert_icon(Icon::Play, true));
    assert!(!should_invert_icon(Icon::Play, false));
}

/// 计算渲染结果中非透明像素的包围盒 (min_x, min_y, max_x, max_y)
fn content_bbox(data: &IconData) -> (u32, u32, u32, u32) {
    let mut min_x = data.width;
    let mut min_y = data.height;
    let mut max_x = 0;
    let mut max_y = 0;
    for y in 0..data.height {
        for x in 0..data.width {
            let idx = ((y * data.width + x) * 4 + 3) as usize;
            if data.rgba[idx] > 0 {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x + 1);
                max_y = max_y.max(y + 1);
            }
        }
    }
    (min_x, min_y, max_x, max_y)
}

/// 非正方形图标（20x10）渲染到 24x24 画布：等比缩放后水平铺满、垂直居中
#[test]
fn test_render_svg_centers_non_square_icon() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10" viewBox="0 0 20 10"><rect x="0" y="0" width="20" height="10" fill="#ff0000"/></svg>"##;
    let data = render_svg(svg, 24, 24).expect("渲染失败");
    // scale = min(24/20, 24/10) = 1.2 → 内容 24x12，y 方向居中：offset = (24-12)/2 = 6
    assert_eq!(content_bbox(&data), (0, 6, 24, 18));
}

/// 高瘦图标（10x20）渲染到 24x24 画布：垂直铺满、水平居中
#[test]
fn test_render_svg_centers_tall_icon() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="20" viewBox="0 0 10 20"><rect x="0" y="0" width="10" height="20" fill="#00ff00"/></svg>"##;
    let data = render_svg(svg, 24, 24).expect("渲染失败");
    // scale = min(24/10, 24/20) = 1.2 → 内容 12x24，x 方向居中：offset = (24-12)/2 = 6
    assert_eq!(content_bbox(&data), (6, 0, 18, 24));
}

/// 正方形图标（24x24）渲染到 24x24 画布：铺满、无留白
#[test]
fn test_render_svg_square_icon_fills_canvas() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24"><rect x="0" y="0" width="24" height="24" fill="#0000ff"/></svg>"##;
    let data = render_svg(svg, 24, 24).expect("渲染失败");
    assert_eq!(content_bbox(&data), (0, 0, 24, 24));
}

/// 真实图标文件：note-tie（20x11.75 非正方形）渲染后内容在 24x24 中垂直居中
#[test]
fn test_render_real_note_tie_icon_is_centered() {
    let svg_data = super::bytes(Icon::Tie);
    let data = render_svg(svg_data, 24, 24).expect("渲染失败");
    let (min_x, min_y, max_x, max_y) = content_bbox(&data);
    // 内容必须完全落在 24x24 画布内（不裁剪、不越界）
    assert!(max_x <= 24 && max_y <= 24);
    // 等比缩放后 content 高度 < 24，应垂直居中：上下留白相等
    let top = min_y;
    let bottom = 24 - max_y;
    assert_eq!(top, bottom, "内容应垂直居中: top={top}, bottom={bottom}");
    // 水平方向应铺满或居中（note-tie 宽高比接近 20:11.75，按 24x24 缩放后可能贴边）
    let left = min_x;
    let right = 24 - max_x;
    assert_eq!(left, right, "内容应水平居中: left={left}, right={right}");
}

/// 全量回归：所有内置 SVG 都能解析并渲染，防止批量修改后出现非法文件
#[test]
fn test_all_icon_svgs_parse_and_render() {
    // 直接触发宏生成的缓存构建：任一 SVG 解析失败会缺失条目（构建函数内部只打日志不 panic）
    let cache = build_icon_cache();

    // 枚举驱动断言（与 define_icons! 宏同源，新增图标自动纳入检查）：
    // 逐个变体验证，失败时精确定位到具体图标，不再依赖手工维护的数量魔法数。
    // （修复背景：新增 upload-to-cloud 图标后数量断言 57→58 未同步导致误报）
    let missing: Vec<Icon> = ALL_ICONS
        .iter()
        .copied()
        .filter(|icon| !cache.contains_key(icon))
        .collect();
    assert!(
        missing.is_empty(),
        "存在无法解析/渲染的 SVG 图标: {missing:?}"
    );

    // 数量兜底：缓存条目必须与枚举条目一致（防止重复路径覆盖等异常产生假阴性）
    assert_eq!(cache.len(), ALL_ICONS.len(), "缓存条目数与图标枚举数不一致");
}

/// 新增的 i2m 悬浮按钮图标（不注册进 Icon 枚举，直接验证 SVG 可解析且输出 32x32 RGBA 纹理）
#[test]
fn test_i2m_button_icons_parse() {
    let check_svg = include_bytes!("../../../../../../resources/icons/toolbar/confirm-check.svg");
    let cross_svg = include_bytes!("../../../../../../resources/icons/toolbar/cancel-cross.svg");
    for svg in [check_svg.as_slice(), cross_svg.as_slice()] {
        let handle = super::svg_handle(svg, 32).expect("i2m 按钮图标应能光栅化");
        match handle {
            iced_core::image::Handle::Rgba {
                width,
                height,
                pixels,
                ..
            } => {
                assert_eq!(width, 32);
                assert_eq!(height, 32);
                assert_eq!(pixels.len(), 32 * 32 * 4);
            }
            other => panic!("应为 RGBA 纹理，实际为 {other:?}"),
        }
    }
}

/// viewBox 扩大 1.5 倍后，内容线性缩小 1/3 且在画布中居中
#[test]
fn test_render_svg_shrunken_viewbox_centers_content() {
    // 20x20 内容放进 30x30 viewBox：渲染到 24x24 应为 16x16 且居中
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="-5 -5 30 30"><rect x="0" y="0" width="20" height="20" fill="#ff0000"/></svg>"##;
    let data = render_svg(svg, 24, 24).expect("渲染失败");
    // scale = 24/30 = 0.8 → 内容 16x16，offset = (24-16)/2 = 4
    assert_eq!(content_bbox(&data), (4, 4, 20, 20));
}

/// 真实播放图标（playback-start）：缩小 1/3 后内容约 12.8x16 且居中
#[test]
fn test_play_icon_shrunk_to_two_thirds_and_centered() {
    let data = render_svg(super::bytes(Icon::Play), 24, 24).expect("渲染失败");
    let (min_x, min_y, max_x, max_y) = content_bbox(&data);
    let w = max_x - min_x;
    let h = max_y - min_y;
    // viewBox 10.39x13.00 → 15.58x19.50：scale = 24/19.50 ≈ 1.23 → 内容约 12.8x16.0
    assert!((w as f32 - 12.78).abs() <= 1.5, "宽度 {w} 应约为 12.78");
    assert!((h as f32 - 16.0).abs() <= 1.5, "高度 {h} 应约为 16.0");
    // 水平/垂直居中
    assert_eq!(
        min_x,
        24 - max_x,
        "内容应水平居中: left={min_x}, right={}",
        24 - max_x
    );
    assert_eq!(
        min_y,
        24 - max_y,
        "内容应垂直居中: top={min_y}, bottom={}",
        24 - max_y
    );
}
