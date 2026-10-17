//! 宽度自动计算：求「内容不被压窄的最小排版宽度」（REF-001 宽度自动计算）
//!
//! # 为什么「自然宽度」只能是下界
//!
//! 对 `Fill` 宽度占多数内容的视图，「自然宽度」不是良定义量：
//!
//! - `Text` 以 `limits.max()` 为折行宽度再取 `min_bounds`
//!   （`iced_core/src/widget/text.rs`）⇒ 长文案量出的宽度**就是传入的 cap**（自证循环）；
//! - `TextInput` / `ProgressBar` 的内在尺寸恒为 `Size::ZERO`
//!   ⇒ `Fill` 宽度量出 **0**。
//!
//! 可定义的只有**下界**：内容驱动的节点不被挤的最小排版宽度。本模块用二分求它，
//! 判据是「内容驱动的节点在窄布局下比超宽基准更小」（[`measure_is_squeezed`]）。
//! 上限（长文本需要的折行宽度）与下限（流体内容的自由度）都由调用方的声明值约束，
//! 因此视图里声明的宽度从「精确值」退化为「区间边界」。
//!
//! 机理与实测依据见父模块文档「为什么宽度必须固定，而不能两个轴都压缩」。

use crate::{Element, Renderer};

use super::layout_at_width;

/// 宽度搜索步数：`(cap - floor) / 2^8` 的分辨率足够（1px 内即停）
const WIDTH_SEARCH_STEPS: u32 = 8;

/// 判定「节点是内容驱动而非流体」的容差（逻辑像素）
const FLUID_TOLERANCE: f32 = 1.0;

/// 判定「节点被压窄」的容差（逻辑像素）
const SQUEEZE_TOLERANCE: f32 = 0.5;

/// 采集两套超宽参考布局时的宽度差（逻辑像素）
const REFERENCE_DELTA: f32 = 160.0;

/// 按先序采集布局树里每个节点的宽度
fn collect_widths(node: &iced_core::layout::Node, out: &mut Vec<f32>) {
    out.push(node.bounds().width);

    for child in node.children() {
        collect_widths(child, out);
    }
}

/// 一次布局的节点宽度画像
fn width_profile(node: &iced_core::layout::Node) -> Vec<f32> {
    let mut widths = Vec::new();
    collect_widths(node, &mut widths);

    widths
}

/// 宽度基准画像：区分**内容驱动**节点与**流体**节点（REF-001 宽度自动计算）
///
/// 做法：在 `wide_width` 与 `wide_width - REFERENCE_DELTA` 两个超宽布局下各采集
/// 一次节点宽度。宽度**随可用宽度变化**的节点是流体（`Fill`：包括被 padding
/// 内缩的容器），宽度**不变**的节点由内容决定（`Fixed` 控件 / 图标 /
/// 不折行的文本）。
///
/// 只用「明显窄于参考宽度」来判内容驱动是不够的：被 padding 内缩的流体容器
/// 也满足那个条件，会制造假阳性（实测：所有自动宽度都被顶到上限）。
pub struct WidthBaseline {
    /// 每个节点在超宽布局下的内容驱动宽度（流体的节点为 `None`）
    widths: Vec<Option<f32>>,
}

/// 采集宽度基准画像；返回 `None` 表示入参非法或结构不稳定
pub fn measure_width_baseline<'a>(
    mut build_view: impl FnMut() -> Element<'a>,
    renderer: &Renderer,
    wide_width: f32,
    max_height: f32,
) -> Option<WidthBaseline> {
    if !wide_width.is_finite() || wide_width <= REFERENCE_DELTA {
        return None;
    }

    let wide = width_profile(&layout_at_width(
        build_view(),
        renderer,
        wide_width,
        max_height,
    )?);
    let narrow = width_profile(&layout_at_width(
        build_view(),
        renderer,
        wide_width - REFERENCE_DELTA,
        max_height,
    )?);

    if wide.len() != narrow.len() {
        return None;
    }

    Some(WidthBaseline {
        widths: wide
            .iter()
            .zip(&narrow)
            .map(|(wide_width, narrow_width)| {
                let tracks_available = (wide_width - narrow_width).abs() > FLUID_TOLERANCE;

                (!tracks_available).then_some(*wide_width)
            })
            .collect(),
    })
}

/// 在 `layout_width` 下是否有**内容驱动**的节点被压窄
///
/// # 为什么不能检测「横向溢出」
///
/// iced **不会产生横向溢出**：`Limits::resolve` 对 `Fixed(n)` 会做
/// `n.min(max)`，而 flex 传给子节点的是**剩余可用宽度**，因此过窄时
/// `Fixed` 控件被直接压窄而不是溢出。实测（批量编辑排版宽度 247）：
/// 声明 `Fixed(120)` 的输入框实际布局宽度只有 **49**，且整棵树没有任何节点
/// 越过父边界——「溢出」作为判据恒为假。
///
/// 可观测的信号是**压窄**：基准里判为内容驱动的节点，在窄布局下比基准更小
/// ⇒ 它被挤了。流体节点不参与判定（它们的宽度本来就是自由参数）。
pub fn measure_is_squeezed<'a>(
    mut build_view: impl FnMut() -> Element<'a>,
    renderer: &Renderer,
    layout_width: f32,
    max_height: f32,
    baseline: &WidthBaseline,
) -> Option<bool> {
    let current = width_profile(&layout_at_width(
        build_view(),
        renderer,
        layout_width,
        max_height,
    )?);

    // 结构不一致（不同状态的视图）：保守判为「没被挤」，交由声明区间兜底
    if current.len() != baseline.widths.len() {
        return Some(false);
    }

    Some(
        current
            .iter()
            .zip(&baseline.widths)
            .any(|(current, baseline_width)| {
                baseline_width
                    .is_some_and(|baseline_width| *current < baseline_width - SQUEEZE_TOLERANCE)
            }),
    )
}

/// 求内容**不被压窄的最小排版宽度**（REF-001 宽度自动计算）
///
/// # 语义
///
/// 返回宽度的**下界**：内容驱动的节点不被挤的最小宽度（判据见
/// [`measure_is_squeezed`]）。
///
/// - 宽度的**下限**（流体内容不会被挤，宽度是自由参数）由调用方的声明值兜底；
/// - 宽度的**上限**（长文本需要的折行宽度）同样由声明值约束。
///   两者之间才是自动量出来的区间，因此声明的宽度从「精确值」退化为「边界」。
///
/// 返回 `None` 表示入参非法；连 `max_width` 都压窄内容时返回 `max_width`
/// （调用方按上限处理，并由尺寸验证测试暴露「声明上限不够用」）。
pub fn measure_min_fitting_width<'a>(
    mut build_view: impl FnMut() -> Element<'a>,
    renderer: &Renderer,
    max_height: f32,
    min_width: f32,
    max_width: f32,
) -> Option<f32> {
    if !min_width.is_finite()
        || min_width <= 0.0
        || !max_width.is_finite()
        || max_width < min_width
        || !max_height.is_finite()
        || max_height <= 0.0
    {
        return None;
    }

    let baseline = measure_width_baseline(&mut build_view, renderer, max_width, max_height)?;

    let mut squeezed_at = |width: f32| -> Option<bool> {
        measure_is_squeezed(&mut build_view, renderer, width, max_height, &baseline)
    };

    // 上限都压窄内容：交给调用方按上限处理（不要谎报一个更小的值）
    if squeezed_at(max_width)? {
        return Some(max_width);
    }

    // 二分：`low` 未知/被压窄，`high` 已知不被压窄
    let mut low = min_width;
    let mut high = max_width;

    for _ in 0..WIDTH_SEARCH_STEPS {
        if high - low <= 1.0 {
            break;
        }

        let mid = (low + high) / 2.0;

        if squeezed_at(mid)? {
            low = mid;
        } else {
            high = mid;
        }
    }

    Some(high)
}
