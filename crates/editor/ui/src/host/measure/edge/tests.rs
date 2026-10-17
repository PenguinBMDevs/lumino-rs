//! 尺子的**反向对照**：证明它抓得住真问题（否则"X/X 干净"没有意义）。
//!
//! 三组对照，一组比一组接近真实：
//! 1. 探针视图放得下 ⇒ 干净（尺子不是恒红）；
//! 2. 探针视图放不下 ⇒ 必红（尺子不是恒绿）；
//! 3. **真实对话框视图**收窄 40% ⇒ 必红（尺子确实看得见产品代码里的控件，
//!    而不是只对探针敏感）。

use super::super::dialog_sizes::headless_renderer;
use super::corners::root_for;
use super::*;
use crate::state::root_state::DialogType;
use iced_core::Length;
use iced_widget::{container, row, space};

/// 两个 Fixed(300) 并排 ⇒ 内容自然宽 600；窗口只给 300 时控件必须被压窄。
fn probe_view() -> Element<'static> {
    container(
        row![
            space()
                .width(Length::Fixed(300.0))
                .height(Length::Fixed(40.0)),
            space()
                .width(Length::Fixed(300.0))
                .height(Length::Fixed(40.0)),
        ]
        .spacing(0),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// 在给定尺寸下跑一遍判据，返回违规清单
fn violations_of<'a>(
    renderer: &Renderer,
    mut build_view: impl FnMut() -> Element<'a>,
    size: Size,
) -> Vec<Violation> {
    let Some(baseline) = baseline(&mut build_view, renderer, size) else {
        return Vec::new();
    };
    let node = layout_at_size(build_view(), renderer, size);
    let current = Profile::collect(&node);

    let mut walker = Walker {
        baseline: &baseline,
        current: &current,
        index: 0,
        violations: Vec::new(),
    };
    walker.run(&node, false, false);
    walker.violations
}

fn violations_at(renderer: &Renderer, size: Size) -> Vec<Violation> {
    violations_of(renderer, probe_view, size)
}

/// 窗口够宽 ⇒ 干净。
#[test]
fn ruler_is_clean_when_window_is_wide_enough() {
    let Some(renderer) = headless_renderer() else {
        eprintln!("跳过：无可用 GPU 适配器");
        return;
    };

    assert!(
        violations_at(&renderer, Size::new(1200.0, 400.0)).is_empty(),
        "窗口 1200 宽时装得下 600 宽的内容，不该报违规"
    );
}

/// 窗口过窄 ⇒ **必须**报违规（这条用例就是这把尺子的价值证明）。
#[test]
fn ruler_detects_squeeze_caused_by_narrow_window() {
    let Some(renderer) = headless_renderer() else {
        eprintln!("跳过：无可用 GPU 适配器");
        return;
    };

    let violations = violations_at(&renderer, Size::new(300.0, 400.0));

    assert!(
        !violations.is_empty(),
        "窗口 300 宽放不下 600 宽的内容，必须报出被压窄——\
         若这里为空，说明尺子恒绿，不能作为门槛"
    );
}

/// 真实对话框视图里的控件，尺子也必须看得见
///
/// 探针只能证明判据会算，这条证明判据**作用在产品代码的视图树上**：把尺寸决策的
/// 结果逐级收窄（60% / 45% / 30%），只要其中任何一档报出违规，就说明尺子看得见
/// 这棵视图树里的控件。三个角落分别代表三类布局——
/// 设置页（`boxed`，两侧定宽标签 + 定宽控件）、云浏览（`boxed`，工具行多按钮）、
/// 找回音轨（`at_least_width`，列表行内长文件名 + 行尾按钮）。
///
/// 为什么要多档：云浏览的工具行在 432 宽时四个按钮仍排得下（实测无违规），
/// 只有真挤到放不下才该红——单档取值会把「确实还装得下」误判成「尺子瞎了」。
#[test]
fn ruler_sees_squeeze_inside_real_dialog_views() {
    let Some(renderer) = headless_renderer() else {
        eprintln!("跳过：无可用 GPU 适配器");
        return;
    };

    const STEPS: [f32; 3] = [0.6, 0.45, 0.3];

    let ui_config = lumino_core::storage::config::UiConfig::default();
    let corners = [
        ("设置[页 2]", DialogType::Settings, "[页 2]"),
        ("CloudBrowser[有列表]", DialogType::CloudBrowser, "[有列表]"),
        ("RecoverTrack[有列表]", DialogType::RecoverTrack, "[有列表]"),
    ];

    let mut blind = Vec::new();

    for (label, dialog_type, suffix) in corners {
        let root = root_for(dialog_type, suffix, &ui_config).expect("角落必须存在");
        let real = decide_dialog_window_size(&root, &renderer, dialog_type, WORK_AREA);

        let caught = STEPS.iter().any(|factor| {
            let narrow = Size::new(real.width * factor, real.height);

            !violations_of(&renderer, || root.view(), narrow).is_empty()
        });

        if !caught {
            blind.push(format!("{label}（原宽 {:.0}）", real.width));
        }
    }

    assert!(
        blind.is_empty(),
        "以下真实视图在最窄一档仍报「干净」——尺子看不见这些控件：{}",
        blind.join("、")
    );
}
