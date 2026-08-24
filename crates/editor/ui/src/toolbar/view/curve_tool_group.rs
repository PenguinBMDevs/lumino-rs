//! 曲线工具组 + 工具选择面板（悬浮层）
//!
//! 采用标准 `Widget::overlay` 把面板锚定在曲线按钮正下方（与 Tooltip / combo_box
//! 的悬浮层同源），布局与命中区全部交由 iced 统一处理。
//!
//! 关键修正（相对此前手写 MenuOverlay 的两处病灶）：
//! 1. 定位使用**按钮自身边界** `content_bounds`，而非视口边界——避免面板高度被误算、
//!    出现"比按钮高一倍"的错位。
//! 2. `Overlay::update` / `Overlay::mouse_interaction` **显式转发**给面板元素——
//!    否则 trait 默认实现"什么都不做"，面板内按钮永远收不到点击（工具切不动）。
//!
//! 面板背景用 `mouse_area(...) .on_press(close)` 包裹（见 tools.rs），点击面板内
//! 空白处即关闭下拉；面板内按钮仍优先响应自身 `on_press`（与右键悬浮面板同源，
//! 不会被 mouse_area 吞掉）。

use iced_core::{
    Clipboard, Element, Event, Layout, Length, Point, Rectangle, Shell, Size, Vector, layout,
    mouse, overlay, renderer, widget,
};

use crate::{Message, Renderer, Theme};

/// 曲线工具组：左侧曲线按钮（含小三角），右侧可选的下拉面板（工具面板 / 画刷下拉）。
pub struct CurveToolGroup<'a> {
    /// 激活时显示的按钮内容（曲线按钮 + 小三角）。
    content: Element<'a, Message, Theme, Renderer>,
    /// 下拉面板（工具面板或画刷下拉），互斥，仅其一存在。
    menu: Option<Element<'a, Message, Theme, Renderer>>,
    /// 面板宽度（用于约束布局 / 越界吸附）。
    menu_width: f32,
}

impl<'a> CurveToolGroup<'a> {
    pub fn new(
        content: Element<'a, Message, Theme, Renderer>,
        menu: Option<Element<'a, Message, Theme, Renderer>>,
        menu_width: f32,
    ) -> Self {
        Self { content, menu, menu_width }
    }
}

impl widget::Widget<Message, Theme, Renderer> for CurveToolGroup<'_> {
    fn children(&self) -> Vec<widget::Tree> {
        let mut trees = vec![widget::Tree::new(&self.content)];
        if let Some(menu) = &self.menu {
            trees.push(widget::Tree::new(menu));
        }
        trees
    }

    fn diff(&self, tree: &mut widget::Tree) {
        let mut children: Vec<&dyn widget::Widget<Message, Theme, Renderer>> =
            vec![self.content.as_widget()];
        if let Some(menu) = &self.menu {
            children.push(menu.as_widget());
        }
        tree.diff_children(&children);
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }

    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        inherited_style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            inherited_style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut widget::Tree,
        layout: Layout<'b>,
        _renderer: &Renderer,
        _viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let menu = self.menu.as_mut()?;
        let menu_tree = &mut tree.children[1];

        let content_bounds = layout.bounds();
        let anchor = layout.position() + translation;

        Some(overlay::Element::new(Box::new(PanelOverlay {
            content_bounds,
            anchor,
            menu,
            tree: menu_tree,
            menu_width: self.menu_width,
        })))
    }
}

/// 面板悬浮层：锚定在曲线按钮正下方，由 iced 负责事件转发与绘制。
struct PanelOverlay<'a, 'b> {
    content_bounds: Rectangle,
    anchor: Point,
    menu: &'b mut Element<'a, Message, Theme, Renderer>,
    tree: &'b mut widget::Tree,
    menu_width: f32,
}

impl overlay::Overlay<Message, Theme, Renderer> for PanelOverlay<'_, '_> {
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        let viewport = Rectangle::with_size(bounds);

        let menu_layout = self.menu.as_widget_mut().layout(
            self.tree,
            renderer,
            &layout::Limits::new(Size::ZERO, viewport.size()),
        );
        let menu_bounds = menu_layout.bounds();

        // 锚定：面板左缘与按钮左缘对齐，顶缘在按钮正下方留 2px 间隙。
        let mut x = self.anchor.x;
        let mut y = self.anchor.y + self.content_bounds.height + 2.0;

        let width = menu_bounds.width.max(self.menu_width);

        // 右越界则左移，吸附视口内。
        if x + width > viewport.x + viewport.width {
            x = (viewport.x + viewport.width - width).max(viewport.x);
        }
        // 下方空间不足（被视口底裁掉）则上移到按钮正上方。
        if y + menu_bounds.height > viewport.y + viewport.height {
            y = (self.anchor.y - menu_bounds.height - 2.0).max(viewport.y);
        }

        layout::Node::with_children(Size::new(width, menu_bounds.height), vec![menu_layout])
            .translate(Vector::new(x, y))
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        inherited_style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.menu.as_widget().draw(
            self.tree,
            renderer,
            theme,
            inherited_style,
            layout.children().next().expect("面板悬浮层必有唯一子节点（菜单元素）"),
            cursor,
            &Rectangle::with_size(Size::INFINITE),
        );
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        // 关键：把事件转发给面板元素，按钮的 on_press 才能被触发。
        self.menu.as_widget_mut().update(
            self.tree,
            event,
            layout.children().next().expect("面板悬浮层必有唯一子节点（菜单元素）"),
            cursor,
            renderer,
            clipboard,
            shell,
            &Rectangle::with_size(Size::INFINITE),
        );
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.menu.as_widget().mouse_interaction(
            self.tree,
            layout.children().next().expect("面板悬浮层必有唯一子节点（菜单元素）"),
            cursor,
            &Rectangle::with_size(Size::INFINITE),
            renderer,
        )
    }
}

impl<'a> From<CurveToolGroup<'a>> for Element<'a, Message, Theme, Renderer> {
    fn from(value: CurveToolGroup<'a>) -> Element<'a, Message, Theme, Renderer> {
        Element::new(value)
    }
}
