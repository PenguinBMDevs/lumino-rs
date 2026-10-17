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
