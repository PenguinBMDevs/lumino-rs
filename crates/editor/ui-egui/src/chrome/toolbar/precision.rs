//! 精度选择器（「精度:」标签 + 下拉框）的宽度度量。
//!
//! 工具栏折叠在**渲染前**决策（见 `lumino_ui::toolbar::overflow`，那时候拿不到
//! egui 的字形度量），所以宽度必须是「不依赖 `Ui` 的纯函数」：绘制与折叠预算都
//! 只调用本模块。两边共用同一结果，才不会出现「按钮变宽但总宽没变」导致压住相邻
//! 控件的问题。
//!
//! 估算口径：按字符类别取 em 上界（内置比例字体在 13/14px 下的实测 advance 上浮
//! 8%~15%，见 `tests/toolbar_precision_bands.rs` 的实测断言），再乘字号。上界保证
//! 绘制时**文本永远放得下**——按钮随文本自动 resize，不需要靠省略号收敛。
//!
//! 用户若在设置里把程序字体（比例族首位）换成异常宽的字体，估算可能被击穿；那种
//! 情况下由绘制侧的 `Label::truncate` 兜底，不会画到按钮外。

use crate::layout;
use crate::theme;

/// 精度下拉按钮宽度：`max(下限, 内边距 + 图标 + 间距 + 文本 + 内边距)`。
///
/// 结果向上取整：切换档位时按钮宽度按整数点变化，不会出现半像素抖动。
pub fn button_width(snap: &str) -> f32 {
    let text = text_width_upper(snap, theme::font::BODY);
    (layout::PRECISION_ICON + layout::PRECISION_ICON_GAP + text + layout::PRECISION_PAD_X * 2.0)
        .max(layout::PRECISION_W)
        .ceil()
}

/// 「精度:」/ `Precision:` 标签的**折叠预算预留**宽度（上界）。
///
/// 文案由生产侧按当前语言给出，长度可变（中文 3 字符 / 英文 10 字符）。绘制侧用
/// 真实度量（更窄），预算用上界——只会给得更宽，不会压到相邻分组。
pub fn label_width(label: &str) -> f32 {
    text_width_upper(label, theme::font::LABEL).ceil()
}

/// 精度选择器总宽度 = 标签预留 + 间距 + 下拉框（不含左侧工具条目的间距）。
pub fn selector_width(label: &str, snap: &str) -> f32 {
    label_width(label) + layout::PRECISION_LABEL_GAP + button_width(snap)
}

/// 单行文本宽度上界（逻辑点）。
///
/// 公开给集成测试：用真实字体度量出的宽度必须 ≤ 本函数的返回值，否则按钮会截断。
pub fn text_width_upper(text: &str, size: f32) -> f32 {
    text.chars().map(char_em).sum::<f32>() * size
}

/// 单字符宽度上界（em）。
///
/// 内置比例字体（Ubuntu-Light + Noto Sans SC 回退）的实测 advance：
/// 数字 0.565 / `/` 0.358 / `.` `:` 0.245 / `B` 0.630 / `C` 0.608 / `P` 0.591 /
/// `m` 0.863 / `a` 0.510 / `r` 0.375 / CJK 1.000。各档上浮 8%~15%，并把 ASCII
/// 未知字符压到 1.0 em、非 ASCII 压到 1.05 em（不依赖具体字体）。
fn char_em(c: char) -> f32 {
    match c {
        '0'..='9' => 0.62,
        '/' => 0.40,
        ' ' | '.' | ',' | ':' | ';' => 0.30,
        'B' => 0.69,
        'C' => 0.67,
        'P' => 0.65,
        'a' => 0.56,
        'c' => 0.51,
        'e' => 0.60,
        'i' => 0.28,
        'm' => 0.94,
        'n' | 'u' => 0.62,
        'o' => 0.64,
        'r' => 0.42,
        's' => 0.47,
        't' => 0.43,
        _ if c.is_ascii() => 1.0,
        _ => 1.05,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 窄文本回落到历史宽度下限，不因为自动 resize 而缩水。
    #[test]
    fn test_button_width_keeps_floor() {
        assert_eq!(button_width(""), layout::PRECISION_W);
        assert_eq!(button_width("/"), layout::PRECISION_W);
        assert!(button_width("1/4") >= layout::PRECISION_W);
    }

    /// 宽度必须随文本单调增长：三连音与自定义 tick 档位只会更宽。
    #[test]
    fn test_button_width_grows_with_text() {
        let quarter = button_width("1/4");
        let triplet = button_width("1/32 3");
        let custom = button_width("Custom 3840");
        let long_custom = button_width("Custom 4294967295");
        assert!(quarter < triplet, "{quarter} 应小于 {triplet}");
        assert!(triplet < custom, "{triplet} 应小于 {custom}");
        assert!(custom < long_custom, "{custom} 应小于 {long_custom}");
    }

    /// 按钮宽度 = 图标 + 间距 + 文本上界 + 左右内边距（向上取整）。
    #[test]
    fn test_button_width_formula() {
        let snap = "1/32 3";
        let text = text_width_upper(snap, theme::font::BODY);
        let expected = (layout::PRECISION_ICON
            + layout::PRECISION_ICON_GAP
            + text
            + layout::PRECISION_PAD_X * 2.0)
            .ceil();
        assert_eq!(button_width(snap), expected);
    }

    /// 校准锚点：内置比例字体下的实测上界结果。
    ///
    /// 改估算表或字形常量必须同步更新这里——否则「按钮忽然变宽/变窄」没人发现。
    #[test]
    fn test_button_width_calibration() {
        assert_eq!(button_width("1/4"), 60.0, "四分音符：比历史 57 只宽 3px");
        assert_eq!(button_width("1/32 3"), 80.0, "三连音档位必须完整显示");
        assert_eq!(button_width("Custom 3840"), 124.0, "自定义 4 位 tick");
    }

    /// 选择器宽度 = 标签 + 间距 + 下拉框；标签变长（中 → 英）整体变宽。
    #[test]
    fn test_selector_width_composition() {
        let zh = selector_width("精度:", "1/4");
        let en = selector_width("Precision:", "1/4");
        assert_eq!(
            zh,
            label_width("精度:") + layout::PRECISION_LABEL_GAP + button_width("1/4")
        );
        assert!(en > zh, "英文标签更长，预留宽度必须跟着变宽");
    }

    /// 未知字形按全角上界估算，不会因为生僻字把按钮画窄。
    #[test]
    fn test_unknown_glyph_is_wide() {
        assert!(text_width_upper("中", 13.0) > text_width_upper("1", 13.0));
        assert!(text_width_upper("\u{1F600}", 13.0) > text_width_upper("1", 13.0));
    }
}
