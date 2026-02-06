---
name: "iced-widget-development"
description: "Comprehensive guide for developing custom widgets in Iced 0.14. Invoke when user needs to create or modify Iced widgets."
---

# Iced 0.14 Widget Development Guide

This skill provides comprehensive guidance for developing custom widgets using Iced 0.14's Widget trait.

## When to Invoke

**ALWAYS invoke this skill when:**
- User wants to create a custom widget in Iced 0.14
- User needs to implement the Widget trait
- User asks about widget state management
- User needs to handle widget events
- User wants to render custom graphics in a widget
- User encounters Widget trait implementation issues

## Core Concepts

### 1. Widget Trait Structure

The Widget trait in Iced 0.14 requires implementing these methods:

```rust
impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for MyWidget {
    // Required methods:
    fn size(&self) -> Size<Length>;
    fn layout(&self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node;
    fn draw(&self, tree: &Tree, renderer: &mut Renderer, theme: &Theme, style: &renderer::Style, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle);
    fn update(&mut self, tree: &mut Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &Renderer, clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, Message>, viewport: &Rectangle);
    fn mouse_interaction(&self, tree: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle, renderer: &Renderer) -> mouse::Interaction;
    fn tag(&self) -> tree::Tag;
    fn state(&self) -> tree::State;
}
```

### 2. State Management with Tree

Iced uses the `Tree` structure to manage widget state:

```rust
// Define your state type
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum MyWidgetState {
    #[default]
    Idle,
    Hover,
    Dragging { start_x: f32, start_y: f32 },
}

// In your widget struct, implement tag() and state():
pub struct MyWidget {
    // ... fields
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for MyWidget {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<MyWidgetState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(MyWidgetState::default())
    }
}
```

### 3. Event Handling Pattern

```rust
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
    let state = tree.state.downcast_mut::<MyWidgetState>();
    let bounds = layout.bounds();

    if let Event::Mouse(mouse_event) = event {
        match mouse_event {
            mouse::Event::ButtonPressed(mouse::Button::Left) => {
                if let Some(position) = cursor.position() {
                    if bounds.contains(position) {
                        *state = MyWidgetState::Dragging {
                            start_x: position.x,
                            start_y: position.y,
                        };
                        shell.request_redraw();
                    }
                }
            }
            mouse::Event::ButtonReleased(mouse::Button::Left) => {
                if matches!(state, MyWidgetState::Dragging { .. }) {
                    *state = MyWidgetState::Idle;
                    shell.request_redraw();
                }
            }
            mouse::Event::CursorMoved { .. } => {
                match *state {
                    MyWidgetState::Dragging { start_x, start_y } => {
                        if let Some(position) = cursor.position() {
                            let delta_x = position.x - start_x;
                            let delta_y = position.y - start_y;
                            // Handle dragging logic
                            shell.publish(Message::MyAction(delta_x, delta_y));
                        }
                    }
                    _ => {
                        let is_hover = if let Some(position) = cursor.position() {
                            bounds.contains(position)
                        } else {
                            false
                        };
                        let new_state = if is_hover {
                            MyWidgetState::Hover
                        } else {
                            MyWidgetState::Idle
                        };
                        if *state != new_state {
                            *state = new_state;
                            shell.request_redraw();
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
```

### 4. Rendering with Quad

```rust
fn draw(
    &self,
    tree: &Tree,
    renderer: &mut Renderer,
    theme: &Theme,
    _style: &renderer::Style,
    layout: Layout<'_>,
    cursor: mouse::Cursor,
    _viewport: &Rectangle,
) {
    let bounds = layout.bounds();
    let palette = theme.extended_palette();
    let state = tree.state.downcast_ref::<MyWidgetState>();

    // Determine color based on state
    let color = match state {
        MyWidgetState::Dragging { .. } => palette.primary.strong.color,
        MyWidgetState::Hover => palette.primary.base.color,
        MyWidgetState::Idle => palette.primary.weak.color,
    };

    // Draw quad
    renderer.fill_quad(
        Quad {
            bounds,
            border: Border {
                radius: 4.0.into(),
                width: 1.0,
                color: palette.strong.color,
            },
            shadow: iced_core::Shadow::default(),
            snap: false,
        },
        Background::Color(color),
    );
}
```

### 5. Mouse Interaction

```rust
fn mouse_interaction(
    &self,
    tree: &Tree,
    _layout: Layout<'_>,
    cursor: mouse::Cursor,
    _viewport: &Rectangle,
    _renderer: &Renderer,
) -> mouse::Interaction {
    let state = tree.state.downcast_ref::<MyWidgetState>();
    match state {
        MyWidgetState::Dragging { .. } => mouse::Interaction::Grabbing,
        MyWidgetState::Hover => mouse::Interaction::Pointer,
        _ => mouse::Interaction::default(),
    }
}
```

## Common Patterns

### Pattern 1: Using Palette Colors

Instead of manually calculating RGB values, use Iced's palette system:

```rust
let palette = theme.extended_palette();

// Use palette hierarchy for consistent theming
palette.primary.strong.color  // Strongest color
palette.primary.base.color    // Base color
palette.primary.weak.color    // Weakest color
palette.background.weak.color
palette.text.base.color
```

### Pattern 2: Clamping Values

Use `.clamp()` instead of `.max().min()`:

```rust
// Before:
let value = (x / max).max(0.0).min(1.0);

// After:
let value = (x / max).clamp(0.0, 1.0);
```

### Pattern 3: Using matches! Macro

Replace simple match expressions with `matches!`:

```rust
// Before:
match value {
    1 | 3 | 5 => true,
    _ => false,
}

// After:
matches!(value, 1 | 3 | 5)
```

### Pattern 4: if let for Single Pattern Matching

Replace single-pattern match with `if let`:

```rust
// Before:
match event {
    Event::Mouse(mouse_event) => {
        // handle mouse event
    }
    _ => {}
}

// After:
if let Event::Mouse(mouse_event) = event {
    // handle mouse event
}
```

### Pattern 5: Collapsible else if

```rust
// Before:
if condition1 {
    // ...
} else {
    if condition2 {
        // ...
    } else {
        // ...
    }
}

// After:
if condition1 {
    // ...
} else if condition2 {
    // ...
} else {
    // ...
}
```

### Pattern 6: Removing Redundant Closures

```rust
// Before:
widget.view(|value| Message::Update(value))

// After:
widget.view(Message::Update)
```

## Best Practices

1. **Always use `#[derive(Default)]`** for state enums instead of manual implementation
2. **Use `#[default]` attribute** to mark the default variant
3. **Downcast state correctly** using `tree.state.downcast_ref()` or `tree.state.downcast_mut()`
4. **Request redraw** when state changes: `shell.request_redraw()`
5. **Publish messages** using `shell.publish()` for parent communication
6. **Use palette colors** for consistent theming
7. **Handle all mouse events** (pressed, released, moved)
8. **Check cursor position** before using it: `if let Some(position) = cursor.position()`
9. **Use bounds.contains(position)** to check if cursor is within widget
10. **Implement proper mouse interaction** states (Idle, Hover, Grabbing, etc.)

## Common Imports

```rust
use iced_core::{Length, Rectangle, Size, mouse, Event, Renderer as _Renderer};
use iced_core::widget::Tree;
use iced_core::layout;
use iced_core::renderer::{self, Quad};
use iced_core::border::Border;
use iced_core::Background;
```

## Example: Complete Widget Implementation

```rust
use crate::{Message, Renderer, Theme, Element};
use iced_core::{Length, Rectangle, Size, mouse, Event, Renderer as _Renderer};
use iced_core::widget::Tree;
use iced_core::layout;
use iced_core::renderer::{self, Quad};
use iced_core::border::Border;
use iced_core::Background;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum MyWidgetState {
    #[default]
    Idle,
    Hover,
    Dragging { start_x: f32, start_y: f32 },
}

pub struct MyWidget<'a> {
    pub on_action: Box<dyn Fn(f32, f32) -> Message + 'a>,
}

impl<'a> MyWidget<'a> {
    pub fn new(on_action: impl Fn(f32, f32) -> Message + 'a) -> Self {
        Self {
            on_action: Box::new(on_action),
        }
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for MyWidget<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        tree.state = self.state();
        layout::Node::new(limits.max())
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let palette = theme.extended_palette();
        let state = tree.state.downcast_ref::<MyWidgetState>();

        let color = match state {
            MyWidgetState::Dragging { .. } => palette.primary.strong.color,
            MyWidgetState::Hover => palette.primary.base.color,
            MyWidgetState::Idle => palette.primary.weak.color,
        };

        renderer.fill_quad(
            Quad {
                bounds,
                border: Border::default(),
                shadow: iced_core::Shadow::default(),
                snap: false,
            },
            Background::Color(color),
        );
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn iced_core::Clipboard,
        shell: &mut iced_core::Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<MyWidgetState>();
        let bounds = layout.bounds();

        if let Event::Mouse(mouse_event) = event {
            match mouse_event {
                mouse::Event::ButtonPressed(mouse::Button::Left) => {
                    if let Some(position) = cursor.position() {
                        if bounds.contains(position) {
                            *state = MyWidgetState::Dragging {
                                start_x: position.x,
                                start_y: position.y,
                            };
                            shell.request_redraw();
                        }
                    }
                }
                mouse::Event::ButtonReleased(mouse::Button::Left) => {
                    if matches!(state, MyWidgetState::Dragging { .. }) {
                        *state = MyWidgetState::Idle;
                        shell.request_redraw();
                    }
                }
                mouse::Event::CursorMoved { .. } => {
                    match *state {
                        MyWidgetState::Dragging { start_x, start_y } => {
                            if let Some(position) = cursor.position() {
                                let delta_x = position.x - start_x;
                                let delta_y = position.y - start_y;
                                shell.publish((self.on_action)(delta_x, delta_y));
                            }
                        }
                        _ => {
                            let is_hover = if let Some(position) = cursor.position() {
                                bounds.contains(position)
                            } else {
                                false
                            };
                            let new_state = if is_hover {
                                MyWidgetState::Hover
                            } else {
                                MyWidgetState::Idle
                            };
                            if *state != new_state {
                                *state = new_state;
                                shell.request_redraw();
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        _layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<MyWidgetState>();
        match state {
            MyWidgetState::Dragging { .. } => mouse::Interaction::Grabbing,
            MyWidgetState::Hover => mouse::Interaction::Pointer,
            _ => mouse::Interaction::default(),
        }
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<MyWidgetState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(MyWidgetState::default())
    }
}

impl<'a> From<MyWidget<'a>> for Element<'a> {
    fn from(widget: MyWidget<'a>) -> Self {
        Element::new(widget)
    }
}
```

## Troubleshooting

### Issue: Widget doesn't update
- Ensure you're calling `shell.request_redraw()` when state changes
- Check that you're modifying state using `tree.state.downcast_mut()`
- Verify that the message is being published correctly

### Issue: Mouse events not working
- Check that bounds are correct in `layout.bounds()`
- Verify cursor position is checked with `if let Some(position) = cursor.position()`
- Ensure you're handling the correct mouse button

### Issue: Colors don't change
- Verify you're using `theme.extended_palette()` correctly
- Check that state is being updated properly
- Ensure you're downcasting state reference correctly

### Issue: Compilation errors with Widget trait
- Make sure you're importing `iced_core::Renderer as _Renderer`
- Check that all required methods are implemented
- Verify generic parameters match your widget's signature
