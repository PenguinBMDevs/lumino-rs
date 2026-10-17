//! REF-001 第三轮：**边缘控件可见性**尺子（核心判据）
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
//! 「流体就流体」。**文字允许让位**（折行/收窄 ≤10%），**控件不许被挤**。
//!
//! # 判据为什么不能靠「越界」
//!
//! iced **不产生越界**：`Limits::resolve` 对 `Fixed(n)` 做 `n.min(max)`，过窄时
//! 控件被**压窄**而不是溢出（实测：批量编辑 @w=247，声明 `Fixed(120)` 的输入框
//! 只有 49）。因此「自然尺寸基准」是必需的：用两套宽松布局（`ref` 与 `ref-160`）
//! 区分流体与内容驱动，再把最终布局与基准逐节点比对。
//!
//! # 模块结构
//!
//! | 文件 | 职责 |
//! |------|------|
//! | `edge`（本文件） | 判据核心：基准、遍历器、单角诊断 |
//! | `edge/corners` | 待检角落清单（**类型 × 视图状态**）与全量扫描 |
//! | `edge/report` | 诊断报告（人看）与 CI 门槛（机器判） |
//! | `edge/tests` | 反向对照：证明尺子抓得住真问题 |

use iced_core::Size;
use iced_core::layout::{Limits, Node};
use iced_core::widget::Tree;

use crate::state::root_state::DialogType;
use crate::{Element, Renderer, Root};

use super::decide_dialog_window_size;
use super::dialog_sizes::WORK_AREA;

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
pub(super) struct Violation {
    /// 先序节点下标（用于定位）
    pub(super) index: usize,
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
        // 例外只给**滚动视口**：`scrollable` 的布局节点是「padding 包裹层 + 唯一内容
        // 子节点」（`iced_widget/src/scrollable.rs` 的 `layout::padded`），内容超出视口
        // 是设计如此，可以滚出来。所以豁免必须同时满足：
        //   a. 容器在该轴**流体**（尺寸随窗口变化，不是内容撑出来的）；
        //   b. 容器**只有一个子节点**——滚动视口的形状。
        // 只判 (a) 会漏：云浏览面板的工具行 `row[pick_list(Fill), 刷新, 断开, 粘贴,
        // 清空剪贴板]` 在窄窗口下工具行整体仍是流体，但四个按钮保持自然宽度**越出**
        // 容器（实测 216 宽时行宽 184、按钮合计逾 300），被 (a) 当成滚动视口放过了
        // ——`ruler_sees_squeeze_inside_real_dialog_views` 就是这么发现它的。
        let bounds = node.bounds();
        let single_child = node.children().len() == 1;
        let mut child_clip_width = clip_width;
        let mut child_clip_height = clip_height;

        for child in node.children() {
            let child_bounds = child.bounds();

            if !clip_width
                && child_bounds.x + child_bounds.width > bounds.width + VIOLATION_TOLERANCE
            {
                if self.baseline.fluid_width[index] && single_child {
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
                if self.baseline.fluid_height[index] && single_child {
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

/// 诊断单个角落：返回最终布局节点与违规清单
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

#[cfg(test)]
mod corners;
#[cfg(test)]
mod report;
#[cfg(test)]
mod tests;
