//! REF-001 第三轮：**边缘控件可见性**诊断（先量后治）
//!
//! # 验收定义（倒推自需求）
//!
//! 「所有最边缘的控件都显示出来」= 在**最终窗口尺寸**下（真实渲染限制，
//! 与 `UserInterface::build` 同口径）：
//!
//! 1. 任何**内容驱动**的控件都不得小于它的自然尺寸（横向被压窄 / 纵向被压扁
//!    = 文字被截、控件被挤成细条）；
//! 2. 任何子节点都不得越出容器边界（= 被容器裁掉）；
//! 3. **滚动视口内部豁免**：滚动区的内容本来就该超出视口，那不是「没显示出来」，
//!    而是「可以滚出来」。
//!
//! 流体容器（`Fill`）本身不参与判定——它的尺寸是自由参数，这正是本需求的
//! 「流体就流体」。
//!
//! # 判据为什么不能靠「越界」
//!
//! iced **不产生越界**：`Limits::resolve` 对 `Fixed(n)` 做 `n.min(max)`，过窄时
//! 控件被**压窄**而不是溢出（实测：批量编辑 @w=247，声明 `Fixed(120)` 的输入框
//! 只有 49）。因此「自然尺寸基准」是必需的：用两套宽松布局（`ref` 与 `ref-160`）
//! 区分流体与内容驱动，再把最终布局与基准逐节点比对。
//!
//! # 现状
//!
//! 本模块是**诊断工具**：`report_edge_visibility` 打印 17 类的违规清单（不改产品
//! 行为）；`assert_edge_visibility_clean` 是未来的 CI 门槛，修完违规后去掉
//! `#[ignore]` 即可转正（见实施记录 §9 的处置计划）。

use iced_core::Size;
use iced_core::layout::{Limits, Node};
use iced_core::widget::Tree;

use crate::state::root_state::DialogType;
use crate::{Element, Renderer, Root};

use super::decide_dialog_window_size;
use super::dialog_sizes::{ALL_DIALOGS, WORK_AREA, headless_renderer};

/// 判定「流体 / 内容驱动」的容差（逻辑像素）
const FLUID_TOLERANCE: f32 = 1.0;

/// 判定「被压 / 越界」的容差（逻辑像素）
const VIOLATION_TOLERANCE: f32 = 0.5;

/// 宽度收窄的**重排容忍比例**
///
/// 宽度收窄 ≤ 该比例时视为**正常重排**（滚动条占位、尾部文字折行），不算"控件被压"。
/// 依据实测（REF-001 验收返工）：
/// - 交互区块容器 393.5 → 379.5（−3.6%）：尾部文字由 1 行折成 2 行，**控件未被压**；
/// - 音频页输入框 80 → 36（−55%）、刷新按钮 80 → 36（−55%）：真正的控件被压。
///
/// 10% 恰好把两者分开，且不会放过中度收窄（200 → 179 仍会报）。
const REFLOW_TOLERANCE_RATIO: f32 = 0.1;

/// 两套参考布局的尺寸差（逻辑像素）
const REFERENCE_DELTA: f32 = 160.0;

/// 宽松宽度基准（远远大于任何真实窗口）
///
/// **必须远大于任何真实窗口**：iced 的 flex 按声明顺序把「剩余可用宽度」递减地交给
/// 子节点；若参考宽度本身不够宽，排在长文本后面的按钮**在基准里就已经被挤扁**——
/// 基准与实测同样坏，尺子就报"干净"（假绿）。
const WIDE_REFERENCE_WIDTH: f32 = 10_000.0;

/// 宽松高度基准的增量（逻辑像素）：用于高度轴的「流体 / 内容驱动」判别
const TALL_REFERENCE_DELTA: f32 = 4_000.0;

/// 设置面板的页索引（多状态覆盖：默认只测第 0 页，会漏掉其它页的控件）
const SETTINGS_PAGES: [usize; 10] = [0, 1, 2, 3, 4, 5, 6, 7, 8, crate::settings::MENU_INDEX_ABOUT];

/// 违规的轴向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    Width,
    Height,
}

/// 违规类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// 内容驱动节点被压得比自然尺寸小
    Squeezed,
    /// 子节点越出容器边界（被裁）
    OutOfBounds,
}

/// 单个违规
#[derive(Debug, Clone, Copy)]
struct Violation {
    /// 先序节点下标（用于定位）
    index: usize,
    axis: Axis,
    kind: Kind,
    expected: f32,
    actual: f32,
}

/// 一次布局的节点尺寸画像（先序）
#[derive(Debug, Default, Clone)]
struct Profile {
    widths: Vec<f32>,
    heights: Vec<f32>,
}

impl Profile {
    fn collect(node: &Node) -> Self {
        let mut profile = Self::default();
        profile.push(node);

        profile
    }

    fn push(&mut self, node: &Node) {
        let bounds = node.bounds();
        self.widths.push(bounds.width);
        self.heights.push(bounds.height);

        for child in node.children() {
            self.push(child);
        }
    }

    fn len(&self) -> usize {
        self.widths.len()
    }
}

/// 宽松布局下的自然尺寸 + 两轴的流体标记
struct Baseline {
    /// 宽度自然值：取自「宽松宽度 × **实测高度**」——高度必须与实测一致，
    /// 否则滚动条出现/消失会让内容区宽差 10 余像素，制造假阳性。
    natural_widths: Vec<f32>,
    /// 宽松宽度布局下的高度（仅用于判断「宽度变化是不是换行造成的重排」）
    wide_heights: Vec<f32>,
    /// 高度自然值：取自「实测宽度 × 宽松高度」
    natural_heights: Vec<f32>,
    fluid_width: Vec<bool>,
    fluid_height: Vec<bool>,
}

impl Baseline {
    fn len(&self) -> usize {
        self.natural_widths.len()
    }
}

/// 在给定窗口尺寸下按**真实渲染口径**跑一次布局
fn layout_at_size(view: Element<'_>, renderer: &Renderer, size: Size) -> Node {
    let mut element = view;
    let mut tree = Tree::new(&element);
    let limits = Limits::new(Size::ZERO, size);

    element.as_widget_mut().layout(&mut tree, renderer, &limits)
}

/// 采集基准：**按轴**取宽松布局，区分流体与内容驱动
///
/// 宽度轴与高度轴分别用两套参考：
/// - 宽度：`(宽松宽, 实测高)` 与 `(宽松宽-160, 实测高)`——高度固定，滚动条状态与
///   实测一致，避免"基准里没有滚动条、实测里有"造成的十几像素假阳性；
/// - 高度：`(实测宽, 实测高+4000)` 与 `(…+3840)`——宽度固定，同理。
fn baseline<'a>(
    mut build_view: impl FnMut() -> Element<'a>,
    renderer: &Renderer,
    real: Size,
) -> Option<Baseline> {
    let wide = Profile::collect(&layout_at_size(
        build_view(),
        renderer,
        Size::new(WIDE_REFERENCE_WIDTH, real.height),
    ));
    let narrower = Profile::collect(&layout_at_size(
        build_view(),
        renderer,
        Size::new(WIDE_REFERENCE_WIDTH - REFERENCE_DELTA, real.height),
    ));
    let tall = Profile::collect(&layout_at_size(
        build_view(),
        renderer,
        Size::new(real.width, real.height + TALL_REFERENCE_DELTA),
    ));
    let shorter = Profile::collect(&layout_at_size(
        build_view(),
        renderer,
        Size::new(
            real.width,
            real.height + TALL_REFERENCE_DELTA - REFERENCE_DELTA,
        ),
    ));

    if wide.len() != narrower.len()
        || wide.len() != tall.len()
        || wide.len() != shorter.len()
        || wide.widths.is_empty()
    {
        return None;
    }

    Some(Baseline {
        fluid_width: wide
            .widths
            .iter()
            .zip(&narrower.widths)
            .map(|(a, b)| (a - b).abs() > FLUID_TOLERANCE)
            .collect(),
        fluid_height: tall
            .heights
            .iter()
            .zip(&shorter.heights)
            .map(|(a, b)| (a - b).abs() > FLUID_TOLERANCE)
            .collect(),
        natural_widths: wide.widths,
        wide_heights: wide.heights,
        natural_heights: tall.heights,
    })
}

/// 宽度收窄到多少才算「被压」：绝对容差与重排比例取大
fn squeeze_threshold(natural_width: f32) -> f32 {
    VIOLATION_TOLERANCE.max(natural_width * REFLOW_TOLERANCE_RATIO)
}

/// 逐节点比对最终布局与基准，收集违规
struct Walker<'a> {
    baseline: &'a Baseline,
    current: &'a Profile,
    index: usize,
    violations: Vec<Violation>,
}

impl<'a> Walker<'a> {
    fn run(&mut self, node: &Node, clip_width: bool, clip_height: bool) {
        let index = self.index;
        self.index += 1;

        if index >= self.baseline.len() || index >= self.current.len() {
            return;
        }

        // 1) 被压窄 / 被压扁（内容驱动节点不得小于自然尺寸）
        //
        // 例外：**换行**。文本在窄窗口里折行后宽度自然变小、高度变大——那不是被压，
        // 是重排。判据取「宽度变小 **且** 高度没长」，且高度要跟**同一次宽松宽度
        // 布局**比（与高度轴的基准不是同一套：宽度基准固定用实测高度）。
        let height_grew =
            self.current.heights[index] > self.baseline.wide_heights[index] + VIOLATION_TOLERANCE;

        if !clip_width
            && !self.baseline.fluid_width[index]
            && self.current.widths[index]
                < self.baseline.natural_widths[index]
                    - squeeze_threshold(self.baseline.natural_widths[index])
            && !height_grew
        {
            self.violations.push(Violation {
                index,
                axis: Axis::Width,
                kind: Kind::Squeezed,
                expected: self.baseline.natural_widths[index],
                actual: self.current.widths[index],
            });
        }

        if !clip_height
            && !self.baseline.fluid_height[index]
            && self.current.heights[index]
                < self.baseline.natural_heights[index] - VIOLATION_TOLERANCE
        {
            self.violations.push(Violation {
                index,
                axis: Axis::Height,
                kind: Kind::Squeezed,
                expected: self.baseline.natural_heights[index],
                actual: self.current.heights[index],
            });
        }

        // 2) 子节点越出容器边界（= 被裁）
        //
        // 例外：容器在**该轴是流体**（高度/宽度随窗口变化）却装不下内容 ⇒ 那是滚动
        // 视口（`scrollable` 的内容本来就该超出视口，可以滚出来），不是「没显示」。
        // 只有**内容驱动**的容器装不下子节点，才说明内容被裁掉。
        let bounds = node.bounds();
        let mut child_clip_width = clip_width;
        let mut child_clip_height = clip_height;

        for child in node.children() {
            let child_bounds = child.bounds();

            if !clip_width
                && child_bounds.x + child_bounds.width > bounds.width + VIOLATION_TOLERANCE
            {
                if self.baseline.fluid_width[index] {
                    child_clip_width = true;
                } else {
                    self.violations.push(Violation {
                        index,
                        axis: Axis::Width,
                        kind: Kind::OutOfBounds,
                        expected: bounds.width,
                        actual: child_bounds.x + child_bounds.width,
                    });
                    child_clip_width = true;
                }
            }

            if !clip_height
                && child_bounds.y + child_bounds.height > bounds.height + VIOLATION_TOLERANCE
            {
                if self.baseline.fluid_height[index] {
                    child_clip_height = true;
                } else {
                    self.violations.push(Violation {
                        index,
                        axis: Axis::Height,
                        kind: Kind::OutOfBounds,
                        expected: bounds.height,
                        actual: child_bounds.y + child_bounds.height,
                    });
                    child_clip_height = true;
                }
            }
        }

        for child in node.children() {
            self.run(child, child_clip_width, child_clip_height);
        }
    }
}

/// 诊断单个对话框：返回最终布局节点与违规清单
fn edge_report(
    root: &Root,
    renderer: &Renderer,
    dialog_type: DialogType,
) -> (Option<Node>, Vec<Violation>) {
    let window_size = decide_dialog_window_size(root, renderer, dialog_type, WORK_AREA);
    if window_size.width <= 0.0 || window_size.height <= 0.0 {
        return (None, Vec::new());
    }

    let Some(baseline) = baseline(|| root.view(), renderer, window_size) else {
        return (None, Vec::new());
    };

    // 与真实渲染同口径的一次布局：既取尺寸画像，也用于遍历（只算一次）
    let node = layout_at_size(root.view(), renderer, window_size);
    let current = Profile::collect(&node);

    let mut walker = Walker {
        baseline: &baseline,
        current: &current,
        index: 0,
        violations: Vec::new(),
    };
    walker.run(&node, false, false);

    (Some(node), walker.violations)
}

/// 一个待检角落
struct Case {
    name: String,
    window_size: Size,
    node: Option<Node>,
    violations: Vec<Violation>,
}

/// 扫一遍所有角落：每个对话框类型 + **设置面板逐页**
///
/// 设置面板是唯一有「多页内容」的对话框（10 页），只测默认的第 0 页会漏掉
/// 其它页的控件——这正是验收反馈里"设置界面部分控件"漏检的原因。
fn scan_all(renderer: &Renderer, ui_config: &lumino_core::storage::config::UiConfig) -> Vec<Case> {
    let mut results = Vec::new();

    for dialog_type in ALL_DIALOGS {
        let mut root = Root::new_dialog_with_config(&ui_config.theme, dialog_type, ui_config);

        if dialog_type == DialogType::Settings {
            for page in SETTINGS_PAGES {
                root.settings.selected_menu_index = page;
                let window_size =
                    decide_dialog_window_size(&root, renderer, dialog_type, WORK_AREA);
                let (node, violations) = edge_report(&root, renderer, dialog_type);
                results.push(Case {
                    name: format!("Settings[页 {page}]"),
                    window_size,
                    node,
                    violations,
                });
            }
            continue;
        }

        let window_size = decide_dialog_window_size(&root, renderer, dialog_type, WORK_AREA);
        let (node, violations) = edge_report(&root, renderer, dialog_type);
        results.push(Case {
            name: format!("{dialog_type:?}"),
            window_size,
            node,
            violations,
        });
    }

    results
}

/// 调试定位：把某先序下标节点的「祖先链 + 兄弟序号」渲染成可读路径
///
/// 形如 `#24[1/2](688×90) → #126[0/5](464×32) → #130[2/5](392×18)`，
/// 用来把违规下标映射回视图源码里那一行 `row![...]`。
fn node_path(node: &Node, target: usize, counter: &mut usize, path: &mut Vec<String>) -> bool {
    let index = *counter;
    *counter += 1;

    let bounds = node.bounds();
    // 找到自己在兄弟里的序号
    path.push(format!(
        "#{index}({:.0}×{:.0})",
        bounds.width, bounds.height
    ));

    if index == target {
        return true;
    }

    for child in node.children() {
        if node_path(child, target, counter, path) {
            return true;
        }
    }

    path.pop();
    false
}

fn describe(node: &Node, target: usize) -> String {
    let mut path = Vec::new();
    let mut counter = 0;
    let _ = node_path(node, target, &mut counter, &mut path);

    path.join(" → ")
}

/// 诊断报告：打印全部角落（17 类 + 设置面板 10 页）的边缘可见性现状
#[test]
#[ignore = "诊断报告；需要时用 --ignored 运行"]
fn report_edge_visibility() {
    let Some(renderer) = headless_renderer() else {
        eprintln!("跳过：无可用 GPU 适配器（边缘可见性诊断需要真实文本度量）");
        return;
    };

    let ui_config = lumino_core::storage::config::UiConfig::default();
    let results = scan_all(&renderer, &ui_config);
    let clean = results
        .iter()
        .filter(|case| case.violations.is_empty())
        .count();

    println!("REF-001 边缘控件可见性诊断（工作区 {WORK_AREA:?}）:");
    for case in &results {
        let Case {
            name,
            window_size,
            node,
            violations,
        } = case;

        if violations.is_empty() {
            println!(
                "  {name}: 干净（窗口 {:.0}×{:.0}）",
                window_size.width, window_size.height
            );
            continue;
        }

        println!(
            "  {name}: {} 处违规（窗口 {:.0}×{:.0}）",
            violations.len(),
            window_size.width,
            window_size.height
        );
        for violation in violations.iter().take(6) {
            let path = node
                .as_ref()
                .map_or_else(String::new, |node| describe(node, violation.index));
            println!(
                "      {axis:?} {kind:?}: 自然 {expected:.1} → 实际 {actual:.1}",
                axis = violation.axis,
                kind = violation.kind,
                expected = violation.expected,
                actual = violation.actual,
            );
            println!("        路径 {path}");
        }
    }

    println!("  —— {} 个角落中 {clean} 个干净", results.len());
}

/// CI 门槛：17 类对话框都不许有边缘可见性违规
///
/// 「所有最边缘的控件都显示出来」的可执行定义。覆盖 17 类对话框 +
/// **设置面板全部 10 页**（只测默认页会漏检其它页的控件——那正是一次验收反馈）。
#[test]
fn assert_edge_visibility_clean() {
    let Some(renderer) = headless_renderer() else {
        eprintln!("跳过：无可用 GPU 适配器");
        return;
    };

    let ui_config = lumino_core::storage::config::UiConfig::default();
    let mut failures = Vec::new();

    for case in scan_all(&renderer, &ui_config) {
        let Case {
            name,
            node,
            violations,
            ..
        } = case;

        if violations.is_empty() {
            continue;
        }

        let detail = violations
            .iter()
            .take(3)
            .map(|violation| {
                let path = node
                    .as_ref()
                    .map_or_else(String::new, |node| describe(node, violation.index));
                format!(
                    "{axis:?} {kind:?} 自然 {expected:.1} → 实际 {actual:.1} @ {path}",
                    axis = violation.axis,
                    kind = violation.kind,
                    expected = violation.expected,
                    actual = violation.actual,
                )
            })
            .collect::<Vec<_>>()
            .join("；");

        failures.push(format!("{name}: {} 处（{detail}）", violations.len()));
    }

    assert!(
        failures.is_empty(),
        "以下位置存在边缘控件被压窄/被裁：\n  {}",
        failures.join("\n  ")
    );
}

#[cfg(test)]
mod tests {
    //! 尺子的**反向对照**：证明它抓得住真问题（否则"17/17 干净"没有意义）。

    use super::*;
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

    fn violations_at(renderer: &Renderer, size: Size) -> Vec<Violation> {
        let baseline = baseline(probe_view, renderer, size).expect("宽松基准必须可测");
        let node = layout_at_size(probe_view(), renderer, size);
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
}
