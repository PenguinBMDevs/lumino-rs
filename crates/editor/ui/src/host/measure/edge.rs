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

/// 两套参考布局的尺寸差（逻辑像素）
const REFERENCE_DELTA: f32 = 160.0;

/// 诊断用的宽松参考尺寸（两轴都足够大）
const REFERENCE_SIZE: Size = Size::new(1920.0, 1400.0);

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
    natural: Profile,
    fluid_width: Vec<bool>,
    fluid_height: Vec<bool>,
}

/// 在给定窗口尺寸下按**真实渲染口径**跑一次布局
fn layout_at_size(view: Element<'_>, renderer: &Renderer, size: Size) -> Node {
    let mut element = view;
    let mut tree = Tree::new(&element);
    let limits = Limits::new(Size::ZERO, size);

    element.as_widget_mut().layout(&mut tree, renderer, &limits)
}

/// 采集基准：两套宽松布局对比，区分流体与内容驱动
fn baseline<'a>(
    mut build_view: impl FnMut() -> Element<'a>,
    renderer: &Renderer,
) -> Option<Baseline> {
    let wide = Profile::collect(&layout_at_size(build_view(), renderer, REFERENCE_SIZE));
    let narrow = Profile::collect(&layout_at_size(
        build_view(),
        renderer,
        Size::new(
            REFERENCE_SIZE.width - REFERENCE_DELTA,
            REFERENCE_SIZE.height - REFERENCE_DELTA,
        ),
    ));

    if wide.len() != narrow.len() || wide.widths.is_empty() {
        return None;
    }

    Some(Baseline {
        fluid_width: wide
            .widths
            .iter()
            .zip(&narrow.widths)
            .map(|(a, b)| (a - b).abs() > FLUID_TOLERANCE)
            .collect(),
        fluid_height: wide
            .heights
            .iter()
            .zip(&narrow.heights)
            .map(|(a, b)| (a - b).abs() > FLUID_TOLERANCE)
            .collect(),
        natural: wide,
    })
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

        if index >= self.baseline.natural.len() || index >= self.current.len() {
            return;
        }

        // 1) 被压窄 / 被压扁（内容驱动节点不得小于自然尺寸）
        //
        // 例外：**换行**。文本在窄窗口里折行后宽度自然变小、高度变大——那不是被压，
        // 是重排。判据取「宽度变小 **且** 高度没长」才算被压窄。
        let height_grew = self.current.heights[index]
            > self.baseline.natural.heights[index] + VIOLATION_TOLERANCE;

        if !clip_width
            && !self.baseline.fluid_width[index]
            && self.current.widths[index]
                < self.baseline.natural.widths[index] - VIOLATION_TOLERANCE
            && !height_grew
        {
            self.violations.push(Violation {
                index,
                axis: Axis::Width,
                kind: Kind::Squeezed,
                expected: self.baseline.natural.widths[index],
                actual: self.current.widths[index],
            });
        }

        if !clip_height
            && !self.baseline.fluid_height[index]
            && self.current.heights[index]
                < self.baseline.natural.heights[index] - VIOLATION_TOLERANCE
        {
            self.violations.push(Violation {
                index,
                axis: Axis::Height,
                kind: Kind::Squeezed,
                expected: self.baseline.natural.heights[index],
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

/// 诊断单个对话框：返回违规清单（空 = 边缘控件都显示得出来）
fn edge_violations(root: &Root, renderer: &Renderer, dialog_type: DialogType) -> Vec<Violation> {
    let window_size = decide_dialog_window_size(root, renderer, dialog_type, WORK_AREA);
    if window_size.width <= 0.0 || window_size.height <= 0.0 {
        return Vec::new();
    }

    let Some(baseline) = baseline(|| root.view(), renderer) else {
        return Vec::new();
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

    walker.violations
}

/// 诊断报告：打印 17 类对话框的边缘可见性现状（人工排查用）
#[test]
#[ignore = "诊断报告；需要时用 --ignored 运行"]
fn report_edge_visibility() {
    let Some(renderer) = headless_renderer() else {
        eprintln!("跳过：无可用 GPU 适配器（边缘可见性诊断需要真实文本度量）");
        return;
    };

    let ui_config = lumino_core::storage::config::UiConfig::default();
    let mut clean = 0;

    println!("REF-001 边缘控件可见性诊断（工作区 {WORK_AREA:?}）:");
    for dialog_type in ALL_DIALOGS {
        let root = Root::new_dialog_with_config(&ui_config.theme, dialog_type, &ui_config);
        let window_size = decide_dialog_window_size(&root, &renderer, dialog_type, WORK_AREA);
        let violations = edge_violations(&root, &renderer, dialog_type);

        if violations.is_empty() {
            clean += 1;
            println!(
                "  {dialog_type:?}: 干净（窗口 {:.0}×{:.0}）",
                window_size.width, window_size.height
            );
        } else {
            println!(
                "  {dialog_type:?}: {} 处违规（窗口 {:.0}×{:.0}）",
                violations.len(),
                window_size.width,
                window_size.height
            );
            for violation in violations.iter().take(4) {
                println!(
                    "      #{idx} {axis:?} {kind:?}: 自然 {expected:.1} → 实际 {actual:.1}",
                    idx = violation.index,
                    axis = violation.axis,
                    kind = violation.kind,
                    expected = violation.expected,
                    actual = violation.actual,
                );
            }
        }
    }

    println!("  —— 17 类中 {clean} 类干净");
}

/// CI 门槛：17 类对话框都不许有边缘可见性违规
///
/// 「所有最边缘的控件都显示出来」的可执行定义。当前 17/17 干净；
/// 将来某个对话框内容变长把窗口撑破时，这里会变红而不是让用户看到被压扁的控件。
#[test]
fn assert_edge_visibility_clean() {
    let Some(renderer) = headless_renderer() else {
        eprintln!("跳过：无可用 GPU 适配器");
        return;
    };

    let ui_config = lumino_core::storage::config::UiConfig::default();
    let mut failures = Vec::new();

    for dialog_type in ALL_DIALOGS {
        let root = Root::new_dialog_with_config(&ui_config.theme, dialog_type, &ui_config);
        let violations = edge_violations(&root, &renderer, dialog_type);

        if violations.is_empty() {
            continue;
        }

        let detail = violations
            .iter()
            .take(3)
            .map(|violation| {
                format!(
                    "#{idx} {axis:?} {kind:?} 自然 {expected:.1} → 实际 {actual:.1}",
                    idx = violation.index,
                    axis = violation.axis,
                    kind = violation.kind,
                    expected = violation.expected,
                    actual = violation.actual,
                )
            })
            .collect::<Vec<_>>()
            .join("；");

        failures.push(format!(
            "{dialog_type:?}: {} 处（{detail}）",
            violations.len()
        ));
    }

    assert!(
        failures.is_empty(),
        "以下对话框存在边缘控件被压窄/被裁：\n  {}",
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
        let baseline = baseline(probe_view, renderer).expect("宽松基准必须可测");
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
