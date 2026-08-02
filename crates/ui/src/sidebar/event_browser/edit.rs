//! 事件浏览器浮动编辑弹窗状态。
//!
//! Canvas 内无法弹出原生 widget，因此 popup 以 Canvas 叠加层形式绘制。
//! 本模块负责维护弹窗状态、键盘输入与确认/取消逻辑。

use iced_core::keyboard::key::Named;
use iced_core::keyboard::{Event as KeyEvent, Key};
use lumino_note_core::event::ScaleType;
use lumino_ui_core::sidebar_event::{EditRequest, TextEventKind};

/// 弹窗类型。
#[derive(Clone, Debug)]
pub(crate) enum PopupState {
    /// 数值输入。
    Number {
        title: String,
        value: String,
        request: EditRequest,
    },
    /// 位置输入（小节 / 小节内 tick）。
    Position {
        title: String,
        bar: String,
        tick_in_bar: String,
        focused: u8,
        request: EditRequest,
    },
    /// 文本输入。
    Text {
        title: String,
        value: String,
        request: EditRequest,
    },
    /// 下拉选择。
    Choice {
        title: String,
        options: Vec<String>,
        selected: usize,
        request: EditRequest,
    },
}

/// 弹窗事件处理结果。
pub(crate) enum PopupAction {
    /// 保持弹窗打开（状态可能已更新）。
    Stay(PopupState),
    /// 用户确认，返回待解析的 `(EditRequest, 文本值)`。
    Confirm((EditRequest, String)),
    /// 用户取消。
    Cancel,
}

impl PopupState {
    /// 根据 `EditRequest` 与当前单元格文本创建弹窗。
    pub fn from_request(request: EditRequest, current_text: &str) -> Option<Self> {
        if request.is_position_edit() {
            let (bar, tick) = split_position(current_text);
            Some(PopupState::Position {
                title: title_for(&request),
                bar,
                tick_in_bar: tick,
                focused: 0,
                request,
            })
        } else if request.is_number_edit() {
            Some(PopupState::Number {
                title: title_for(&request),
                value: current_text.to_string(),
                request,
            })
        } else if request.is_text_edit() {
            Some(PopupState::Text {
                title: title_for(&request),
                value: current_text.to_string(),
                request,
            })
        } else if request.is_choice_edit() {
            let (options, selected) = choice_options(&request, current_text);
            Some(PopupState::Choice {
                title: title_for(&request),
                options,
                selected,
                request,
            })
        } else {
            None
        }
    }

    /// 处理键盘事件。
    pub fn handle_key(self, event: &KeyEvent) -> PopupAction {
        match event {
            KeyEvent::KeyPressed { key, .. } => match key {
                Key::Named(Named::Enter) | Key::Named(Named::Tab) => {
                    PopupAction::Confirm(self.confirm())
                }
                Key::Named(Named::Escape) => PopupAction::Cancel,
                Key::Named(Named::Backspace) => PopupAction::Stay(self.backspace()),
                Key::Named(Named::ArrowLeft) => {
                    if let Some(next) = self.clone().prev_choice() {
                        PopupAction::Stay(next)
                    } else {
                        PopupAction::Stay(self)
                    }
                }
                Key::Named(Named::ArrowRight) => {
                    if let Some(next) = self.clone().next_choice() {
                        PopupAction::Stay(next)
                    } else {
                        PopupAction::Stay(self)
                    }
                }
                Key::Character(ch) => {
                    let chars: Vec<char> = ch.chars().collect();
                    let mut next = self;
                    for c in chars {
                        next = next.push_char(c);
                    }
                    PopupAction::Stay(next)
                }
                _ => PopupAction::Stay(self),
            },
            _ => PopupAction::Stay(self),
        }
    }

    fn push_char(mut self, ch: char) -> Self {
        match &mut self {
            PopupState::Number { value, .. } => {
                if ch.is_ascii_digit() || ch == '.' || ch == '-' {
                    value.push(ch);
                }
            }
            PopupState::Position {
                bar,
                tick_in_bar,
                focused,
                ..
            } => {
                let target = if *focused == 0 { bar } else { tick_in_bar };
                if ch.is_ascii_digit() {
                    target.push(ch);
                }
            }
            PopupState::Text { value, .. } => value.push(ch),
            PopupState::Choice { .. } => {}
        }
        self
    }

    fn backspace(mut self) -> Self {
        match &mut self {
            PopupState::Number { value, .. } => {
                value.pop();
            }
            PopupState::Position {
                bar,
                tick_in_bar,
                focused,
                ..
            } => {
                let target = if *focused == 0 { bar } else { tick_in_bar };
                target.pop();
            }
            PopupState::Text { value, .. } => {
                value.pop();
            }
            PopupState::Choice { .. } => {}
        }
        self
    }

    /// Tab 切换位置弹窗的两个输入框焦点。
    #[allow(dead_code)] // 预留：位置弹窗多字段焦点切换
    pub fn cycle_focus(mut self) -> Self {
        if let PopupState::Position { focused, .. } = &mut self {
            *focused = if *focused == 0 { 1 } else { 0 };
        }
        self
    }

    fn confirm(self) -> (EditRequest, String) {
        match self {
            PopupState::Number { value, request, .. } => (request, value),
            PopupState::Position {
                bar,
                tick_in_bar,
                request,
                ..
            } => (request, format!("{}/{}", bar, tick_in_bar)),
            PopupState::Text { value, request, .. } => (request, value),
            PopupState::Choice {
                options,
                selected,
                request,
                ..
            } => {
                let value = options.get(selected).cloned().unwrap_or_default();
                (request, value)
            }
        }
    }

    /// 确认弹窗，返回 `(EditRequest, 文本值)` 供上层应用。
    pub fn confirm_value(self) -> (EditRequest, String) {
        self.confirm()
    }

    /// 选择上一个选项（Choice 弹窗），返回更新后的状态。
    pub fn prev_choice(self) -> Option<Self> {
        if let PopupState::Choice { selected, .. } = &self
            && *selected > 0
        {
            return Some(self.prev_choice_inner());
        }
        None
    }

    /// 选择下一个选项（Choice 弹窗），返回更新后的状态。
    pub fn next_choice(self) -> Option<Self> {
        if let PopupState::Choice {
            selected, options, ..
        } = &self
            && *selected + 1 < options.len()
        {
            return Some(self.next_choice_inner());
        }
        None
    }

    fn prev_choice_inner(mut self) -> Self {
        if let PopupState::Choice { selected, .. } = &mut self
            && *selected > 0
        {
            *selected -= 1;
        }
        self
    }

    fn next_choice_inner(mut self) -> Self {
        if let PopupState::Choice {
            selected, options, ..
        } = &mut self
            && *selected + 1 < options.len()
        {
            *selected += 1;
        }
        self
    }

    pub fn title(&self) -> &str {
        match self {
            PopupState::Number { title, .. }
            | PopupState::Position { title, .. }
            | PopupState::Text { title, .. }
            | PopupState::Choice { title, .. } => title,
        }
    }

    pub fn value_text(&self) -> String {
        match self {
            PopupState::Number { value, .. } => value.clone(),
            PopupState::Position {
                bar,
                tick_in_bar,
                focused,
                ..
            } => {
                if *focused == 0 {
                    format!("{}|/{}", bar, tick_in_bar)
                } else {
                    format!("{}/|{}", bar, tick_in_bar)
                }
            }
            PopupState::Text { value, .. } => value.clone(),
            PopupState::Choice {
                options, selected, ..
            } => options.get(*selected).cloned().unwrap_or_default(),
        }
    }
}

fn title_for(request: &EditRequest) -> String {
    let label = match request {
        EditRequest::AutoTick { .. } => "Tick",
        EditRequest::AutoValue { .. } => "Value",
        EditRequest::AutoShape { .. } => "Shape",
        EditRequest::NoteStartTick { .. } => "Start Tick",
        EditRequest::NoteEndTick { .. } => "End Tick",
        EditRequest::NoteGate { .. } => "Gate",
        EditRequest::NoteKey { .. } => "Key",
        EditRequest::NoteVelocity { .. } => "Velocity",
        EditRequest::TimeSigTick { .. } => "Tick",
        EditRequest::TimeSigNumerator { .. } => "Numerator",
        EditRequest::TimeSigDenominator { .. } => "Denominator",
        EditRequest::KeySigTick { .. } => "Tick",
        EditRequest::KeySigRoot { .. } => "Root",
        EditRequest::KeySigScale { .. } => "Scale",
        EditRequest::PcTick { .. } => "Tick",
        EditRequest::PcProgram { .. } => "Program",
        EditRequest::TextEventTick { kind, .. } => match kind {
            TextEventKind::Marker => "Marker Tick",
            TextEventKind::ConductorLyrics | TextEventKind::Lyrics { .. } => "Lyrics Tick",
            TextEventKind::ConductorChord | TextEventKind::Chord { .. } => "Chord Tick",
        },
        EditRequest::TextEventText { kind, .. } => match kind {
            TextEventKind::Marker => "Marker Text",
            TextEventKind::ConductorLyrics | TextEventKind::Lyrics { .. } => "Lyrics Text",
            TextEventKind::ConductorChord | TextEventKind::Chord { .. } => "Chord Text",
        },
        _ => "Edit",
    };
    format!("Edit {}", label)
}

fn split_position(text: &str) -> (String, String) {
    let mut parts = text.splitn(2, '/');
    let bar = parts.next().unwrap_or("1").to_string();
    let tick = parts.next().unwrap_or("0").to_string();
    (bar, tick)
}

fn choice_options(request: &EditRequest, current_text: &str) -> (Vec<String>, usize) {
    match request {
        EditRequest::KeySigScale { .. } => {
            let options: Vec<String> = ScaleType::ALL.iter().map(scale_name).collect();
            let selected = options.iter().position(|s| s == current_text).unwrap_or(0);
            (options, selected)
        }
        EditRequest::AutoShape { .. } => {
            let options = vec!["Step".to_string(), "Curve".to_string()];
            let selected = if current_text == "Curve" { 1 } else { 0 };
            (options, selected)
        }
        _ => (Vec::new(), 0),
    }
}

fn scale_name(scale: &ScaleType) -> String {
    match scale {
        ScaleType::Major => "Major".to_string(),
        ScaleType::Minor => "Minor".to_string(),
        ScaleType::Dorian => "Dorian".to_string(),
        ScaleType::Phrygian => "Phrygian".to_string(),
        ScaleType::Lydian => "Lydian".to_string(),
        ScaleType::Mixolydian => "Mixolydian".to_string(),
        ScaleType::Aeolian => "Aeolian".to_string(),
        ScaleType::Locrian => "Locrian".to_string(),
        ScaleType::HarmonicMinor => "HarmonicMinor".to_string(),
        ScaleType::MelodicMinor => "MelodicMinor".to_string(),
    }
}
