//! REF-001：对话框内容尺寸测量（**打开时测量一次**，不做运行时持续跟踪）
//!
//! # 为什么需要这个入口
//!
//! 对话框窗口尺寸原先硬编码在 `lumino-dialog` 的 `DialogWindow::new`
//! （17 种 `DialogType` 各一组 `(width, height)`），而对话框视图根一律是
//! `Length::Fill`——「内容改了但数字没回填」就会裁剪或留白。
//!
//! iced 0.14 提供了一条**不需要改任何视图根声明**的测量通道：
//! [`Limits::width`] 传 `Length::Shrink` 时只置 `compression` 标志，
//! [`Limits::resolve`] 随即把 `Fill`/`FillPortion` 退化为内容的**内在尺寸**
//! （`iced_core/src/layout/limits.rs`）。
//!
//! # ⚠️ 调用侧必须遵守的尺寸口径（REF-001 §5.1 决策 D3）
//!
//! - **高度可信**：`Fill` 退化为内在尺寸，且结果天然被 `cap` 钳住
//!   （`resolve` 内部 `.min(max)`），因此「不超工作区」不需要额外 clamp。
//! - **宽度不可信**，两个原因：
//!   1. `TextInput` 的内在尺寸恒为 `Size::ZERO`
//!      （`text_input.rs` 的 `resolve(self.width, height, Size::ZERO)`），
//!      `ProgressBar` 同理（`progress_bar.rs` 的 `layout::atomic`）⇒ `Fill`
//!      宽度会量出 **0**；
//!   2. `Text` 以 `limits.max()` 为折行宽度再取 `min_bounds`
//!      （`iced_core/src/widget/text.rs`）⇒ 长文案量出的宽度**就是传入的 cap**
//!      （自证循环）。
//!
//!   所以宽度侧必须由**视图声明的偏好宽度**决定，调用时把它作为 `cap.width`
//!   传入（它同时充当文本折行宽度），**不要把返回值的宽度当作内容宽度**。
//! - 含**无界滚动区**的对话框（`scrollable` 的滚动轴取内容全长）在压缩测量下
//!   高度恒等于 `cap`，因此必须在尺寸声明里给出 `max_height`，由
//!   [`Host::measure_dialog_window_size`] 夹住——**不需要改视图**（视图保持
//!   纯 `Fill`，滚动区在用户放大窗口时仍会随之增长）。反例与正例见本文件
//!   `tests` 中的契约测试
//!   `contract_scrollable_without_cap_eats_cap_height` /
//!   `contract_scrollable_with_cap_clamps_to_preferred`。
//! - `space().height(Length::Fill)` 这类**柔性撑位**会在测量期吃掉整个上限并把
//!   其后的兄弟压成 0（契约测试
//!   `contract_trailing_fixed_sibling_after_fill_is_zeroed`）。窗口改为贴合内容
//!   后这类撑位已无意义，视图侧应改用固定间距（见 `view::draw_settings_dialog`）。
//!
//! 返回 `Size::ZERO` 表示**测量不可用**（入参非法，或布局结果非有限/非正）。
//! 调用方必须回退到种子尺寸，**绝不允许因此不显示窗口**。

use iced_core::layout::Limits;
use iced_core::widget::Tree;
use iced_core::{Length, Size};

use crate::view::dialog_preferred_size;
use crate::{Element, Renderer};

use super::Host;

/// 尺寸是否可用（非零、有限、为正）
fn usable(size: Size) -> bool {
    size.width.is_finite() && size.height.is_finite() && size.width > 0.0 && size.height > 0.0
}

/// 阻断「文本按 0 宽折行」的测量核心：**固定排版宽度 + 压缩高度**
///
/// # 为什么宽度必须固定，而不能两个轴都压缩
///
/// 直觉做法是给两个轴都传 `Length::Shrink`（让 `Fill` 全部退化为内在尺寸），
/// 但那样**宽度会塌成 0**，进而让文本按 0 宽折行、量出虚高到荒谬的内容高度。
///
/// 机理在 iced 的 flex 布局：当同一层的子节点**横向全是 `Fill`** 时
/// （`view_dialog` 的自定义标题栏包裹层正是 `column![标题栏(Fill), 内容(Fill)]`），
/// `cross` 初值 0 且 pass 1 不会更新它，pass 2 又把 `cross` 当作子节点的
/// `max_width`（`iced_core/src/layout/flex.rs`）⇒ 子树拿到的可用宽度是 0。
/// 实测（阶段 F）：保存确认对话框的提示文字被量成 208px 高、内存监控的说明文字
/// 被量成 337px 高，多个对话框的高度虚高到工作区上限（1040）。
///
/// 因此正确口径是：**宽度 = 视图声明的排版宽度（`min = max`，硬约束）**，
/// **高度 = 压缩**（`Fill` 退化为内在尺寸）。这样文本按声明宽度折行，
/// 高度才是真实内容高度。
///
/// [`Host`] 与单测共用本函数，保证测试口径与生产口径不漂移。
pub fn measure_content_height(
    view: Element<'_>,
    renderer: &Renderer,
    layout_width: f32,
    max_height: f32,
) -> f32 {
    puffin::profile_scope!("dialog_measure_content");

    if !layout_width.is_finite()
        || layout_width <= 0.0
        || !max_height.is_finite()
        || max_height <= 0.0
    {
        return 0.0;
    }

    // 与渲染路径同源的视图树；`Tree` 新建后即丢弃 ⇒ 对渲染的控件状态零副作用。
    let mut element = view;
    let mut tree = Tree::new(&element);

    // 宽度硬约束（`min = max = layout_width`，compression.width = false）；
    // 高度压缩（`Length::Shrink` ⇒ `Fill` 退化为内在尺寸）。
    let limits = Limits::new(
        Size::new(layout_width, 0.0),
        Size::new(layout_width, max_height),
    )
    .height(Length::Shrink);

    let size = element
        .as_widget_mut()
        .layout(&mut tree, renderer, &limits)
        .size();

    if !size.height.is_finite() || size.height <= 0.0 {
        return 0.0;
    }

    size.height
}

impl Host {
    /// 对话框窗口的推荐**逻辑尺寸**（REF-001 决策 D3：宽声明、高测量）
    ///
    /// 规则：`宽 = 声明宽度`；`高 = clamp(测量高度, 声明下限, 声明上限, 工作区上限)`。
    ///
    /// - 宽度取视图侧的声明（[`dialog_preferred_size::dialog_size_hint`]），
    ///   它同时充当**文本折行宽度**（测量时作为硬约束传入，见
    ///   [`measure_content_height`]）；
    /// - 高度由测量驱动，再用声明的上下限兜底：
    ///   - 上限：含无界滚动区的对话框必填（否则高度顶满工作区）；
    ///   - 下限：内容随状态变化、开局态未必最高的对话框必填。
    ///
    /// `work_area` 为窗口所在显示器的**逻辑**工作区（不可用时传 `Size::ZERO`）。
    /// **测量与夹取共用同一个上限**（[`dialog_preferred_size::measurement_cap`]），
    /// 否则内容会在夹取那一步被裁掉。
    ///
    /// 返回 `Size::ZERO` 表示**测量不可用**（`DialogType::None`、宽度未声明、
    /// 或内容高度非正/非有限），调用方必须回退种子尺寸并**照常显示窗口**。
    pub fn measure_dialog_window_size(&self, work_area: Size) -> Size {
        let hint = dialog_preferred_size::dialog_size_hint(self.root.state.dialog_type);

        if !hint.width.is_finite() || hint.width <= 0.0 {
            return Size::ZERO;
        }

        let cap = dialog_preferred_size::measurement_cap(hint, work_area);
        let measured_height =
            measure_content_height(self.root.view(), &self.render_ctx.renderer, hint.width, cap);

        if !usable(Size::new(hint.width, measured_height)) {
            tracing::warn!(
                "对话框内容测量不可用: {:?} 宽度={:.1} 上限={cap:.1} 实测高度={measured_height:.1}，调用方应回退种子尺寸",
                self.root.state.dialog_type,
                hint.width
            );
            return Size::ZERO;
        }

        dialog_preferred_size::recommend_window_size(
            hint,
            work_area,
            Size::new(hint.width, measured_height),
        )
    }
}

#[cfg(test)]
mod dialog_sizes;
#[cfg(test)]
mod tests {
    //! iced 0.14 布局语义的**契约测试**。
    //!
    //! 这些用例锁定「压缩限制 = 内容内在尺寸」这一机制，以及三条会破坏该机制的
    //! 反例。它们与 `docs/2026-10-16-REF-001-统一弹窗窗口尺寸决策调研.md` 附录 A
    //! 的独立探针逐条对应——iced 升级若改变 compression 语义，这里会立刻变红，
    //! 而不是等到某个对话框在用户面前裁剪。
    //!
    //! 用 `()`（iced 内置 null 渲染器）：结构断言不需要真实字体度量，
    //! 因此这些用例**不需要 GPU 适配器**，不会在 CI 软渲染环境被跳过。
    //! （文本度量相关的结论必须用真实渲染器验证，见调研文档 §1.3。）

    use super::*;
    use iced_core::{Element, Layout, Theme};
    use iced_widget::{column, container, row, scrollable, space};

    /// 探针元素：`()` = iced 内置 null 渲染器。
    type Probe<'a> = Element<'a, (), Theme, ()>;

    const CAP: Size = Size::new(800.0, 600.0);

    fn compressed(cap: Size) -> Limits {
        Limits::new(Size::ZERO, cap)
            .width(Length::Shrink)
            .height(Length::Shrink)
    }

    /// 跑一次真实布局，返回根节点尺寸。
    fn root_size(element: &mut Probe<'_>, limits: &Limits) -> Size {
        let mut tree = Tree::new(&*element);
        element
            .as_widget_mut()
            .layout(&mut tree, &(), limits)
            .size()
    }

    fn approx(actual: f32, expected: f32) -> bool {
        (actual - expected).abs() < 0.5
    }

    fn assert_size(actual: Size, expected: (f32, f32), what: &str) {
        assert!(
            approx(actual.width, expected.0) && approx(actual.height, expected.1),
            "{what}: 期望 {expected:?}，实测 ({:.1}, {:.1})",
            actual.width,
            actual.height
        );
    }

    /// 根 `Fill×Fill` + row[120×40, 90×30]，spacing 10，padding 12。
    fn basic() -> Probe<'static> {
        container(
            row![
                space()
                    .width(Length::Fixed(120.0))
                    .height(Length::Fixed(40.0)),
                space()
                    .width(Length::Fixed(90.0))
                    .height(Length::Fixed(30.0)),
            ]
            .spacing(10),
        )
        .padding(12)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    /// 契约 1：普通限制下根 `Fill×Fill` 拿到的就是 cap（= 现状：窗口尺寸决定一切）。
    #[test]
    fn contract_plain_limits_fill_root_takes_cap() {
        let mut element = basic();
        let size = root_size(&mut element, &Limits::new(Size::ZERO, CAP));
        assert_size(size, (800.0, 600.0), "普通限制下 Fill 根应铺满 cap");
    }

    /// 契约 2（REF-001 的核心机制）：压缩限制下**根仍是 `Fill×Fill`** 也能拿到
    /// 内容自然尺寸 ⇒ 不需要给 17 个视图根改成 Shrink。
    #[test]
    fn contract_compressed_limits_measure_intrinsic_size() {
        let mut element = basic();
        let size = root_size(&mut element, &compressed(CAP));
        // 220 = 120 + 10 + 90；64 = 40 + 2×12
        assert_size(size, (244.0, 64.0), "压缩限制应量出内容自然尺寸");
    }

    /// 契约 3：`space().width(Fill)` 这类「零内在」推位占位塌成 0，
    /// 不撑大测量结果（右推按钮的常见写法是安全的）。
    #[test]
    fn contract_fill_spacer_collapses_to_zero() {
        let mut element: Probe<'static> = container(
            column![
                space()
                    .width(Length::Fixed(200.0))
                    .height(Length::Fixed(20.0)),
                row![
                    space().width(Length::Fill),
                    space()
                        .width(Length::Fixed(90.0))
                        .height(Length::Fixed(30.0)),
                ],
            ]
            .width(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

        let size = root_size(&mut element, &compressed(CAP));
        assert_size(size, (200.0, 50.0), "Fill 占位应塌成 0，宽度取内容最大值");
    }

    /// 契约 4：显式 `Fixed` 会**复位**该轴的 compression，其子树内的 `Fill`
    /// 仍铺满 Fixed 值——所以「固定尺寸子区域」在测量下不会被压塌。
    #[test]
    fn contract_fixed_child_resets_compression() {
        let mut element: Probe<'static> = container(
            row![
                container(space().width(Length::Fill).height(Length::Fill))
                    .width(Length::Fixed(300.0))
                    .height(Length::Fixed(40.0)),
                space()
                    .width(Length::Fixed(50.0))
                    .height(Length::Fixed(20.0)),
            ]
            .spacing(10),
        )
        .padding(8)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

        let size = root_size(&mut element, &compressed(CAP));
        // 376 = 300 + 10 + 50 + 2×8；56 = 40 + 2×8
        assert_size(size, (376.0, 56.0), "Fixed 子树内的 Fill 应铺满 Fixed 值");
    }

    /// 契约 5（反例）：滚动区**没有上限**时，滚动轴取内容全长再被 cap 夹住
    /// ⇒ 高度恒等于 cap。这就是「含滚动区的对话框必须声明 preferred 上限」的
    /// 直接依据。
    #[test]
    fn contract_scrollable_without_cap_eats_cap_height() {
        let items: Vec<Probe<'static>> = (0..50)
            .map(|_| space().height(Length::Fixed(20.0)).into())
            .collect();

        let mut element: Probe<'static> = container(scrollable(column(items)).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .into();

        let size = root_size(&mut element, &compressed(CAP));
        assert!(
            approx(size.height, CAP.height),
            "无上限的滚动区必须把高度顶到 cap（实测 {:.1}），否则本契约失效——\
             说明 iced 语义已变，需重新评估 REF-001 的 preferred 上限方案",
            size.height
        );
    }

    /// 契约 6（正例）：滚动区套 `container(...).max_height(pref)` 后，
    /// 高度 = min(内容, pref)，**长内容被夹住**。
    #[test]
    fn contract_scrollable_with_cap_clamps_to_preferred() {
        let items: Vec<Probe<'static>> = (0..50)
            .map(|_| {
                space()
                    .width(Length::Fixed(180.0))
                    .height(Length::Fixed(20.0))
                    .into()
            })
            .collect();

        let mut element: Probe<'static> = container(
            container(scrollable(column(items)).height(Length::Fill))
                .width(Length::Fill)
                .max_height(400.0),
        )
        .padding(10)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

        let size = root_size(&mut element, &compressed(CAP));
        // 420 = 400 + 2×10
        assert_size(size, (200.0, 420.0), "滚动区应被 preferred 上限夹住");
    }

    /// 契约 7（正例）：同上但内容比 preferred 短 ⇒ 仍然贴合内容（不会撑到 pref）。
    #[test]
    fn contract_scrollable_with_cap_hugs_short_content() {
        let items: Vec<Probe<'static>> = (0..15)
            .map(|_| {
                space()
                    .width(Length::Fixed(180.0))
                    .height(Length::Fixed(20.0))
                    .into()
            })
            .collect();

        let mut element: Probe<'static> = container(
            container(scrollable(column(items)).height(Length::Fill))
                .width(Length::Fill)
                .max_height(400.0),
        )
        .padding(10)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

        let size = root_size(&mut element, &compressed(CAP));
        // 320 = 15×20 + 2×10；200 = 180 + 2×10
        assert_size(size, (200.0, 320.0), "短内容应贴合自身高度而非 preferred");
    }

    /// 契约 8（反例，顺序敏感）：`column[固定, Fill 主轴, 固定]` 中，
    /// **柔性节点之后的固定兄弟会被压成 0 高**——所以「底部操作栏」必须放在
    /// 有上限的滚动区之后，否则测量会把按钮行算成 0 高。
    #[test]
    fn contract_trailing_fixed_sibling_after_fill_is_zeroed() {
        let items: Vec<Probe<'static>> = (0..50)
            .map(|_| {
                space()
                    .width(Length::Fixed(180.0))
                    .height(Length::Fixed(20.0))
                    .into()
            })
            .collect();

        let mut element: Probe<'static> = container(
            column![
                space().height(Length::Fixed(20.0)),
                scrollable(column(items)).height(Length::Fill),
                space().height(Length::Fixed(40.0)),
            ]
            .width(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

        let mut tree = Tree::new(&element);
        let node = element
            .as_widget_mut()
            .layout(&mut tree, &(), &compressed(CAP));
        let layout = Layout::new(&node);

        // 结构：container → column → [固定20, scrollable, 固定40]
        let column = layout.children().next().expect("column 节点缺失");
        let children: Vec<Size> = column
            .children()
            .map(|child| child.bounds().size())
            .collect();
        assert_eq!(children.len(), 3, "column 应有 3 个子节点");

        assert!(
            approx(children[2].height, 0.0),
            "Fill 主轴节点之后的固定兄弟必须被压成 0 高（实测 {:.1}）——\
             若不再成立，说明 iced 的 flex compression 语义已变，\
             REF-001 的「底部操作栏」约束需重新评估",
            children[2].height
        );
    }

    /// 契约 9（反例）：只压缩一个轴时，另一个轴的 `Fill` 会吃掉 cap
    /// ⇒ 测量必须两个轴都压缩。
    #[test]
    fn contract_single_axis_compression_takes_cap() {
        let mut element: Probe<'static> = container(
            space()
                .width(Length::Fixed(120.0))
                .height(Length::Fixed(40.0)),
        )
        .padding(12)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

        let size = root_size(
            &mut element,
            &Limits::new(Size::ZERO, CAP).width(Length::Shrink),
        );
        assert!(
            approx(size.height, CAP.height),
            "只压缩宽度时高度必须吃掉 cap（实测 {:.1}），两轴都必须压缩",
            size.height
        );
    }
}
