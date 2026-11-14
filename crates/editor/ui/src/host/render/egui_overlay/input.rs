//! egui 输入缓冲：指针 / 滚轮 / 键盘 / 焦点。
//!
//! 从 `Host` 拆出（Plan B：输入所有权归运行时 / 窗口）。主窗口、对话框、
//! 进度窗各自持有一个实例，`Host::handle_events` 只负责翻译，不再保存状态。
//!
//! 只收「影响交互」的事件；连续指针移动只保留最后一次，避免一帧内堆积成百上千
//! 个中间位置。

use egui;
use winit::event::WindowEvent;

/// egui 输入缓冲：只收「影响显示态」的事件（悬停 / 滚轮 / 修饰键 / 焦点）。
///
/// 与 旧版 的事件队列同策略：连续指针移动只保留最后一次，避免一帧内堆积
/// 成百上千个中间位置。
pub struct OverlayInput {
    /// 待提交给 egui 的事件。
    events: Vec<egui::Event>,
    /// 当前修饰键状态（滚轮/按键事件需要随附）。
    modifiers: egui::Modifiers,
    /// 窗口是否持有焦点（`RawInput::focused`）。
    focused: bool,
    /// 最近一次指针位置（逻辑点），鼠标按键事件需要。
    pub pointer: Option<egui::Pos2>,
    /// 上一帧 egui 是否想要键盘输入（文本编辑 / 输入框获得焦点）。
    wants_keyboard: bool,
}

impl Default for OverlayInput {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            modifiers: egui::Modifiers::default(),
            // 与 egui 的 `RawInput::focused` 默认值一致：窗口可见即可交互。
            focused: true,
            pointer: None,
            wants_keyboard: false,
        }
    }
}

impl OverlayInput {
    /// 把一条 winit 窗口事件翻译成 egui 事件。
    ///
    /// `fallback_pointer`：`MouseInput` 到达时 `pointer` 可能仍为 `None`
    /// （启动后 / `CursorLeft` 后 / 自动化注入的首击）。用宿主已记录的逻辑光标
    /// 补一次，保证 egui 一定收到这次按下。
    ///
    /// 返回 `true` 表示这是输入事件，调用方应请求一次重绘。
    pub fn feed(
        &mut self,
        event: &WindowEvent,
        scale: f32,
        fallback_pointer: Option<egui::Pos2>,
    ) -> bool {
        use winit::event::MouseScrollDelta;

        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer_moved(egui::pos2(
                    position.x as f32 / scale,
                    position.y as f32 / scale,
                ));
            }
            WindowEvent::CursorLeft { .. } => {
                self.pointer_left();
            }
            WindowEvent::MouseWheel { delta, .. } => match delta {
                MouseScrollDelta::LineDelta(x, y) => {
                    self.wheel(egui::MouseWheelUnit::Line, egui::vec2(*x, *y));
                }
                MouseScrollDelta::PixelDelta(position) => {
                    self.wheel(
                        egui::MouseWheelUnit::Point,
                        egui::vec2(position.x as f32, position.y as f32) / scale,
                    );
                }
            },
            WindowEvent::MouseInput { state, button, .. } => {
                let Some(button) = pointer_button(*button) else {
                    return false;
                };
                if self.pointer.is_none()
                    && let Some(pos) = fallback_pointer
                {
                    self.pointer_moved(pos);
                }
                self.button(button, *state == winit::event::ElementState::Pressed);
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let state = modifiers.state();
                let super_key = state.contains(winit::keyboard::ModifiersState::SUPER);
                let control = state.contains(winit::keyboard::ModifiersState::CONTROL);
                self.set_modifiers(egui::Modifiers {
                    alt: state.contains(winit::keyboard::ModifiersState::ALT),
                    ctrl: control,
                    shift: state.contains(winit::keyboard::ModifiersState::SHIFT),
                    mac_cmd: super_key,
                    // egui 语义：command = ⌘（macOS）/ Ctrl（其他平台）。
                    command: if cfg!(target_os = "macos") {
                        super_key
                    } else {
                        control
                    },
                });
            }
            WindowEvent::KeyboardInput { event, .. } => {
                self.key(event);
            }
            WindowEvent::Ime(winit::event::Ime::Commit(text)) => {
                self.text(text);
            }
            WindowEvent::Focused(focused) => {
                self.focus(*focused);
            }
            _ => return false,
        }

        input_requests_repaint(event)
    }

    /// 指针移动到窗口内（逻辑点）。
    pub fn pointer_moved(&mut self, position: egui::Pos2) {
        self.pointer = Some(position);
        if matches!(self.events.last(), Some(egui::Event::PointerMoved(_))) {
            self.events.pop();
        }
        self.events.push(egui::Event::PointerMoved(position));
    }

    /// 指针离开窗口。
    pub fn pointer_left(&mut self) {
        self.pointer = None;
        self.events.push(egui::Event::PointerGone);
    }

    /// 鼠标按键（左/右/中）。
    pub fn button(&mut self, button: egui::PointerButton, pressed: bool) {
        let Some(pos) = self.pointer else {
            return;
        };
        self.events.push(egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: self.modifiers,
        });
    }

    /// 滚轮：单位沿用 winit 语义（行 / 像素），换算交给 egui。
    pub fn wheel(&mut self, unit: egui::MouseWheelUnit, delta: egui::Vec2) {
        self.events.push(egui::Event::MouseWheel {
            unit,
            delta,
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers: self.modifiers,
        });
    }

    /// 键盘事件（文本编辑需要按键与提交文本两路）。
    pub fn key(&mut self, event: &winit::event::KeyEvent) {
        let pressed = event.state == winit::event::ElementState::Pressed;
        if let Some(key) = egui_key(&event.logical_key) {
            self.events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: event.repeat,
                modifiers: self.modifiers,
            });
        }
        // 文本提交只取可打印字符：Ctrl/Alt 组合留给快捷键，不当作输入。
        // 部分平台会给编辑键（退格 / 回车 / Tab）同时附带控制字符文本，
        // 必须过滤——它们由 `Event::Key` 处理，重复插入会污染输入缓冲。
        if pressed
            && !self.modifiers.ctrl
            && !self.modifiers.alt
            && let Some(text) = event.text.as_deref().and_then(printable)
        {
            self.events.push(egui::Event::Text(text));
        }
    }

    /// IME 提交文本（中文输入）。
    pub fn text(&mut self, text: &str) {
        self.events.push(egui::Event::Text(text.to_owned()));
    }

    /// 记录 egui 本帧是否需要键盘（下一帧的按键事件据此决定是否消费）。
    pub fn set_wants_keyboard(&mut self, wants: bool) {
        self.wants_keyboard = wants;
    }

    /// egui 是否正在编辑文本（键盘事件必须只给它，否则会触发 旧版 快捷键）。
    pub fn wants_keyboard(&self) -> bool {
        self.wants_keyboard
    }

    /// 修饰键变化（只有真的变化才入队）。
    pub fn set_modifiers(&mut self, modifiers: egui::Modifiers) {
        if self.modifiers == modifiers {
            return;
        }
        self.modifiers = modifiers;
        self.events.push(egui::Event::ModifiersChanged(modifiers));
    }

    /// 窗口焦点变化。
    pub fn focus(&mut self, focused: bool) {
        if self.focused == focused {
            return;
        }
        self.focused = focused;
        self.events.push(egui::Event::WindowFocused(focused));
    }

    /// 取走本帧事件（缓冲清空）。
    pub fn take(&mut self) -> Vec<egui::Event> {
        std::mem::take(&mut self.events)
    }

    /// 窗口是否持有焦点。
    pub fn focused(&self) -> bool {
        self.focused
    }
}

/// 该窗口事件是否必须驱动一帧 egui。
///
/// **绝不能**把 `RedrawRequested` 算进来：那会让 `RedrawRequested` 的处理
/// 再排一帧，形成无上限的「渲染 → 请求重绘 → 渲染」自旋。`Mailbox`
/// present mode 没有 vsync 上限，事件循环因此永远进不了 `ControlFlow::Wait`，
/// 鼠标消息被 `WM_PAINT` 饿死——真机表现就是「点击要连点很多次才响应」。
pub fn input_requests_repaint(event: &WindowEvent) -> bool {
    matches!(
        event,
        WindowEvent::CursorMoved { .. }
            | WindowEvent::CursorLeft { .. }
            | WindowEvent::MouseWheel { .. }
            | WindowEvent::MouseInput { .. }
            | WindowEvent::ModifiersChanged(_)
            | WindowEvent::KeyboardInput { .. }
            | WindowEvent::Ime(_)
            | WindowEvent::Focused(_)
    )
}

/// winit 鼠标键 → egui 指针键（其余侧键不参与菜单）。
pub fn pointer_button(button: winit::event::MouseButton) -> Option<egui::PointerButton> {
    use winit::event::MouseButton;
    match button {
        MouseButton::Left => Some(egui::PointerButton::Primary),
        MouseButton::Right => Some(egui::PointerButton::Secondary),
        MouseButton::Middle => Some(egui::PointerButton::Middle),
        _ => None,
    }
}

/// winit 逻辑键 → egui 键（文本编辑与常用快捷键；未映射的键不转发）。
pub fn egui_key(key: &winit::keyboard::Key) -> Option<egui::Key> {
    use winit::keyboard::{Key, NamedKey};
    Some(match key {
        Key::Named(named) => match named {
            NamedKey::Enter => egui::Key::Enter,
            NamedKey::Tab => egui::Key::Tab,
            NamedKey::Escape => egui::Key::Escape,
            NamedKey::Backspace => egui::Key::Backspace,
            NamedKey::Delete => egui::Key::Delete,
            NamedKey::Insert => egui::Key::Insert,
            NamedKey::Home => egui::Key::Home,
            NamedKey::End => egui::Key::End,
            NamedKey::PageUp => egui::Key::PageUp,
            NamedKey::PageDown => egui::Key::PageDown,
            NamedKey::ArrowLeft => egui::Key::ArrowLeft,
            NamedKey::ArrowRight => egui::Key::ArrowRight,
            NamedKey::ArrowUp => egui::Key::ArrowUp,
            NamedKey::ArrowDown => egui::Key::ArrowDown,
            NamedKey::Space => egui::Key::Space,
            _ => return None,
        },
        Key::Character(text) => {
            let ch = text.chars().next()?.to_ascii_lowercase();
            match ch {
                'a' => egui::Key::A,
                'b' => egui::Key::B,
                'c' => egui::Key::C,
                'd' => egui::Key::D,
                'e' => egui::Key::E,
                'f' => egui::Key::F,
                'g' => egui::Key::G,
                'h' => egui::Key::H,
                'i' => egui::Key::I,
                'j' => egui::Key::J,
                'k' => egui::Key::K,
                'l' => egui::Key::L,
                'm' => egui::Key::M,
                'n' => egui::Key::N,
                'o' => egui::Key::O,
                'p' => egui::Key::P,
                'q' => egui::Key::Q,
                'r' => egui::Key::R,
                's' => egui::Key::S,
                't' => egui::Key::T,
                'u' => egui::Key::U,
                'v' => egui::Key::V,
                'w' => egui::Key::W,
                'x' => egui::Key::X,
                'y' => egui::Key::Y,
                'z' => egui::Key::Z,
                '0' => egui::Key::Num0,
                '1' => egui::Key::Num1,
                '2' => egui::Key::Num2,
                '3' => egui::Key::Num3,
                '4' => egui::Key::Num4,
                '5' => egui::Key::Num5,
                '6' => egui::Key::Num6,
                '7' => egui::Key::Num7,
                '8' => egui::Key::Num8,
                '9' => egui::Key::Num9,
                ' ' => egui::Key::Space,
                '-' => egui::Key::Minus,
                '=' => egui::Key::Equals,
                '[' => egui::Key::OpenBracket,
                ']' => egui::Key::CloseBracket,
                '\\' => egui::Key::Backslash,
                ';' => egui::Key::Semicolon,
                '\'' => egui::Key::Quote,
                ',' => egui::Key::Comma,
                '.' => egui::Key::Period,
                '/' => egui::Key::Slash,
                '`' => egui::Key::Backtick,
                _ => return None,
            }
        }
        _ => return None,
    })
}

/// 可打印文本：过滤控制字符（编辑键由 `Event::Key` 处理）。
fn printable(text: &str) -> Option<String> {
    (!text.is_empty() && !text.chars().any(char::is_control)).then(|| text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{input_requests_repaint, printable};
    use winit::event::WindowEvent;

    #[test]
    fn printable_filters_control_characters() {
        assert_eq!(printable("16").as_deref(), Some("16"));
        assert_eq!(printable("中文"), Some("中文".to_owned()));
        assert_eq!(printable("\u{8}"), None, "退格控制字符不得插入");
        assert_eq!(printable("\r"), None, "回车必须在插入前被拦截");
        assert_eq!(printable("\t"), None);
        assert_eq!(printable(""), None);
    }

    /// 重绘自旋回归：`RedrawRequested` 不能再驱动下一帧。
    ///
    /// 曾经的实现对所有窗口事件无条件 `request_redraw()`，`RedrawRequested`
    /// 自己也会再次入列 → 无上限自旋（Mailbox 下没有 vsync 上限）。事件循环
    /// 永远进不了 `ControlFlow::Wait`，鼠标消息被 `WM_PAINT` 饿死，真机表现
    /// 为「点击要连点很多次才响应」。fast-release 比 debug 明显，因为每帧更快。
    #[test]
    fn only_input_events_request_repaint() {
        assert!(
            !input_requests_repaint(&WindowEvent::RedrawRequested),
            "RedrawRequested 绝不能再次请求重绘（会形成自旋）"
        );
        assert!(
            !input_requests_repaint(&WindowEvent::Resized(winit::dpi::PhysicalSize::new(
                1280, 720
            ))),
            "尺寸变化由系统重绘驱动，不需要再排一帧"
        );
        assert!(
            input_requests_repaint(&WindowEvent::Focused(true)),
            "焦点变化要刷新 egui 的 focused 状态"
        );
        assert!(
            input_requests_repaint(&WindowEvent::ModifiersChanged(
                winit::event::Modifiers::default()
            )),
            "修饰键变化要同步给 egui（滚轮/快捷键事件需要随附）"
        );
    }
}
