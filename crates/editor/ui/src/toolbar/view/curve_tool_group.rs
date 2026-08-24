//! 曲线工具组自定义 widget
//!
//! 将「曲线工具按钮 + 右侧小三角」与下拉菜单组合为一个 widget，
//! 利用 iced 的 `overlay` 机制把下拉菜单正确锚定在按钮正下方
//! （与 `combo_box` 同款实现思路）。点击菜单外部区域时关闭下拉。

use crate::{Element, Message, Theme};
use iced_core::overlay;
use iced_core::widget::{Operation, Tree};
use iced_core::{
    Clipboard, Event, Layout, Length, Point, Rectangle, Shell, Size, Vector, Widget,
};
use iced_core::{mouse, renderer, touch};
use iced_widget::core::layout;
use iced_wgpu::Renderer;

/// 曲线工具组：内部为工具按钮行，可选的下方下拉菜单。
pub struct CurveToolGroup<'a> {
    content: Element<'a>,
    menu: Option<Element<'a>>,
    menu_width: f32,
    close_message: Message,
}

impl<'a> CurveToolGroup<'a> {
    /// 构造曲线工具组。
    ///
    /// - `content`：按钮行（曲线按钮 + 小三角）。
    /// - `menu`：下拉菜单元素；为 `None` 时不渲染下拉。
    /// - `menu_width`：下拉菜单宽度（像素），用于约束 overlay 布局。
    /// - `close_message`：点击菜单外部区域时发布的关闭消息。
    pub fn new(
        content: Element<'a>,
        menu: Option<Element<'a>>,
        menu_width: f32,
        close_message: Message,
    ) -> Self {
        Self {
            content,
            menu,
            menu_width,
            close_message,
        }
    }
}

impl<'a> Widget<Message, Theme, Renderer> for CurveToolGroup<'a>
where
    Message: Clone,
{
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn update(
        &mut self,
        tree: &mut Tree,
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

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content
            .as_widget()
            .mouse_interaction(&tree.children[0], layout, cursor, viewport, renderer)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        renderer_style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            renderer_style,
            layout,
            cursor,
            viewport,
        );
    }

    #[allow(clippy::type_complexity)]
    fn overlay<'b>(
        &'b mut self,
        _tree: &'b mut Tree,
        layout: Layout<'b>,
        _renderer: &Renderer,
        _viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        // 仅在下拉菜单存在时渲染 overlay；否则返回 None（不覆盖任何区域）。
        //
        // 关键：必须“借用”菜单而不能 `take()`。iced 在每一帧会多次调用
        // `Widget::overlay`（`update` / `operate` / `draw` 各一次），若此处
        // `take()` 消费菜单，则第一次调用（update）拿走菜单后，draw 阶段的
        // 调用只能拿到 `None`，导致菜单“已布局却从不绘制”（表现为下拉菜单打不开）；
        // 同时 `self.overlay` 残留的旧布局会让 overlay 系统错位（表现为悬停工具时
        // 描述文字框被拉长）。combo_box 采用同样的借用方式。
        let menu = self.menu.as_mut()?;

        let bounds = layout.bounds();
        let position = layout.position() + translation;

        // 必须为菜单构建完整的 widget 树（含其内部子控件的 tree.children），
        // 否则菜单内 mouse_area/button 等访问 tree.children[0] 会越界 panic。
        let menu_tree = Tree::new(&*menu);

        let menu_overlay = MenuOverlay {
            position,
            tree: menu_tree,
            menu,
            target_height: bounds.height,
            width: self.menu_width,
            close_message: self.close_message.clone(),
            viewport: _viewport.clone(),
        };

        Some(overlay::Element::new(Box::new(menu_overlay)))
    }
}

impl<'a> From<CurveToolGroup<'a>> for Element<'a> {
    fn from(widget: CurveToolGroup<'a>) -> Element<'a> {
        Element::new(widget)
    }
}

/// 下拉菜单的 overlay 实现：锚定在触发按钮正下方，点击外部关闭。
struct MenuOverlay<'a, 'b> {
    position: Point,
    tree: Tree,
    /// 借用 `CurveToolGroup` 持有的菜单元素（不拥有，避免 `take` 导致 draw 阶段丢失）。
    menu: &'b mut Element<'a>,
    target_height: f32,
    width: f32,
    close_message: Message,
    /// 视口矩形（窗口尺寸），用于计算下拉可用空间，避免依赖 `Overlay::layout`
    /// 传入的 `bounds`（其含义在不同 iced 版本/调用路径下不确定）。
    viewport: Rectangle,
}

impl<'a, 'b> overlay::Overlay<Message, Theme, Renderer> for MenuOverlay<'a, 'b>
where
    Message: Clone + 'a,
{
    fn layout(&mut self, renderer: &Renderer, _bounds: Size) -> layout::Node {
        // 使用视口尺寸计算可用空间，与 combo_box 的做法一致：
        // 按钮底部到视口底部的剩余高度即下拉菜单可占用的纵向空间。
        let space_below = self.viewport.height - (self.position.y + self.target_height);

        let limits = layout::Limits::new(
            Size::ZERO,
            Size::new(
                self.viewport.width - self.position.x,
                space_below.max(0.0),
            ),
        )
        .width(self.width);

        let node = self
            .menu
            .as_widget_mut()
            .layout(&mut self.tree, renderer, &limits);

        let menu_height = node.size().height;

        // 下方空间不足以容纳菜单时改为在按钮上方展开，避免超出视口底部被裁切。
        let y = if space_below >= menu_height {
            self.position.y + self.target_height
        } else {
            (self.position.y - menu_height).max(0.0)
        };

        // 右侧空间不足以容纳菜单时整体左移，避免超出视口右边界。
        let x = if self.position.x + self.width > self.viewport.width {
            (self.viewport.width - self.width).max(0.0)
        } else {
            self.position.x
        };

        node.move_to(Point::new(x, y))
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
        // 点击菜单外部区域时关闭下拉（菜单项自身的消息仍会正常下发）
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerPressed { .. })
                if !cursor.is_over(layout.bounds()) =>
            {
                shell.publish(self.close_message.clone());
            }
            _ => {}
        }

        self.menu.as_widget_mut().update(
            &mut self.tree,
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            &layout.bounds(),
        );
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.menu.as_widget().mouse_interaction(
            &self.tree,
            layout,
            cursor,
            &layout.bounds(),
            renderer,
        )
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        renderer_style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.menu.as_widget().draw(
            &self.tree,
            renderer,
            theme,
            renderer_style,
            layout,
            cursor,
            &layout.bounds(),
        );
    }

    fn operate(
        &mut self,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.menu
            .as_widget_mut()
            .operate(&mut self.tree, layout, renderer, operation);
    }
}
