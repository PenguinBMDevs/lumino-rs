//! 一次性布局探针：打印某个子树的真实屏幕矩形（`LUMINO_DEBUG_CLIP=1`）。
//!
//! iced 布局不对外暴露 bounds，而「用常量正推」在预览卡被收缩/居中时会错位
//! （实测差 ≈172 逻辑像素）。这里用只透传的包装 widget，在 `layout` 时把
//! 子树的 `Node::bounds()` 打到日志，供 egui 侧按**实测**矩形对齐。

use iced_core::widget::{Operation, Tree, tree};
use iced_core::{Length, Rectangle, Shell, Size, Vector, layout, mouse, overlay, renderer};

use crate::{Element, Message, Renderer, Theme};

/// 最近一次布局得到的矩形（label → 逻辑坐标），供 egui 侧按**实测**对齐。
fn rects() -> &'static std::sync::Mutex<std::collections::HashMap<&'static str, Rectangle>> {
    static RECTS: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<&'static str, Rectangle>>,
    > = std::sync::OnceLock::new();
    RECTS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// 读取某个 label 最近一次的布局矩形（逻辑坐标）。
pub fn last_rect(label: &'static str) -> Option<Rectangle> {
    rects().lock().ok()?.get(label).copied()
}

/// 环境变量开关（只读一次）。
fn enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("LUMINO_DEBUG_CLIP").is_ok_and(|v| v != "0"))
}

/// 用探针包住子树（仅在开关打开时打印，行为完全透传）。
pub fn probe<'a>(label: &'static str, child: impl Into<Element<'a>>) -> Element<'a> {
    RectProbe {
        child: child.into(),
        label,
    }
    .into()
}

impl<'a> From<RectProbe<'a>> for Element<'a> {
    fn from(probe: RectProbe<'a>) -> Self {
        Element::new(probe)
    }
}

/// 透传包装：布局时打印子树矩形。
struct RectProbe<'a> {
    child: Element<'a>,
    label: &'static str,
}

impl<'a> iced_core::Widget<Message, Theme, Renderer> for RectProbe<'a> {
    fn tag(&self) -> tree::Tag {
        self.child.as_widget().tag()
    }

    fn state(&self) -> tree::State {
        self.child.as_widget().state()
    }

    fn children(&self) -> Vec<Tree> {
        self.child.as_widget().children()
    }

    fn diff(&self, tree: &mut Tree) {
        self.child.as_widget().diff(tree);
    }

    fn size(&self) -> Size<Length> {
        self.child.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.child.as_widget_mut().layout(tree, renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: layout::Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.child
            .as_widget_mut()
            .operate(tree, layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced_core::Event,
        layout: layout::Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn iced_core::Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.child.as_widget_mut().update(
            tree, event, layout, cursor, renderer, clipboard, shell, viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: layout::Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.child
            .as_widget()
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: layout::Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        // 布局阶段的 Node::bounds 尚未定位（父节点之后才赋值），
        // draw 收到的 layout 才是最终屏幕矩形；始终记录，打印受开关控制。
        let b = layout.bounds();
        if let Ok(mut map) = rects().lock() {
            map.insert(self.label, b);
        }
        if enabled() {
            tracing::warn!(
                label = self.label,
                x = b.x,
                y = b.y,
                w = b.width,
                h = b.height,
                "DEBUG clip rect"
            );
        }
        self.child
            .as_widget()
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: layout::Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.child
            .as_widget_mut()
            .overlay(tree, layout, renderer, viewport, translation)
    }
}
