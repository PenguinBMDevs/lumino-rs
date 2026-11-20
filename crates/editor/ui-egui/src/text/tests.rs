//! `text` 模块单元测试：省略 / 折行的宽度契约。

use super::*;

/// 在真实字体环境下跑一段 UI 代码（文本度量需要 `Context` 的字体图集）。
fn with_ui<R>(f: impl FnOnce(&Ui) -> R) -> R {
    let ctx = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(600.0, 400.0),
        )),
        ..Default::default()
    };
    let mut result = None;
    let mut call = Some(f);
    let mut output = ctx.run_ui(input, |ui| {
        if let Some(f) = call.take() {
            result = Some(f(ui));
        }
    });
    // 没有渲染后端接收字体图集：显式丢弃纹理增量。
    output.textures_delta.clear();
    result.expect("run_ui 必须执行一次闭包")
}

#[test]
fn short_text_is_untouched() {
    with_ui(|ui| {
        let text = "短文本";
        assert_eq!(truncate_to_width(ui, text, 14.0, 500.0), text);
    });
}

#[test]
fn unlimited_width_keeps_text() {
    with_ui(|ui| {
        let text = "一个不需要截断的长文本";
        assert_eq!(truncate_to_width(ui, text, 14.0, 0.0), text);
        assert_eq!(truncate_to_width(ui, text, 14.0, -1.0), text);
    });
}

#[test]
fn long_ascii_text_gets_ellipsis_within_budget() {
    with_ui(|ui| {
        let text = "a-very-long-file-name-that-does-not-fit-into-the-row.lmpj";
        let max = 120.0;
        let fitted = truncate_to_width(ui, text, 12.0, max);
        assert!(fitted.ends_with(ELLIPSIS), "必须以省略号结尾：{fitted}");
        assert!(
            text_width(ui, &fitted, 12.0) <= max,
            "截断后仍超宽：{fitted}"
        );
        assert!(fitted.chars().count() > 1, "必须保留可识别前缀：{fitted}");
    });
}

#[test]
fn long_cjk_text_gets_ellipsis_within_budget() {
    with_ui(|ui| {
        let text = "这是一个很长的中文连接名称，长到列表行一定放不下";
        let max = 90.0;
        let fitted = truncate_to_width(ui, text, 12.0, max);
        assert!(fitted.ends_with(ELLIPSIS), "必须以省略号结尾：{fitted}");
        assert!(
            text_width(ui, &fitted, 12.0) <= max,
            "截断后仍超宽：{fitted}"
        );
    });
}

#[test]
fn budget_smaller_than_ellipsis_degrades_to_ellipsis() {
    with_ui(|ui| {
        let fitted = truncate_to_width(ui, "任意文本", 12.0, 0.1);
        assert_eq!(fitted, ELLIPSIS);
    });
}

#[test]
fn wrapped_height_grows_as_width_shrinks() {
    with_ui(|ui| {
        let text = "折行高度必须随可用宽度变化：宽时一两行，窄时更多行。";
        let wide = wrapped_height(ui, text, 14.0, 480.0);
        let narrow = wrapped_height(ui, text, 14.0, 120.0);
        assert!(narrow > wide, "窄宽度必须折出更多行：{narrow} <= {wide}");
        assert!(narrow > line_height(ui, 14.0), "窄宽度必须多于一行");
    });
}

#[test]
fn wrapped_height_of_short_text_is_one_line() {
    with_ui(|ui| {
        let one = line_height(ui, 12.0);
        let height = wrapped_height(ui, "短说明", 12.0, 400.0);
        assert!(
            (height - one).abs() < 0.5,
            "短文本应为单行：{height} vs {one}"
        );
    });
}
