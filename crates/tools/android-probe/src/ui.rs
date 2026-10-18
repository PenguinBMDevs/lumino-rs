//! 探针 UI：一眼可判定的四件事——中文渲染、按钮点击（触摸→鼠标合成）、
//! 滑杆拖动（连续触摸）、文本输入（软键盘/IME）。

use iced_wgpu::Renderer;
use iced_widget::{button, column, container, row, slider, text, text_input};
use iced_winit::core::{Alignment, Element, Length, Theme};

/// 探针状态。
#[derive(Debug, Default)]
pub struct Probe {
    count: i32,
    progress: f32,
    input: String,
}

/// 探针消息。
#[derive(Debug, Clone)]
pub enum Message {
    /// 计数 +1（点击/触摸都走这条，用来验证触摸→点击合成）。
    Increment,
    /// 计数 -1。
    Decrement,
    /// 滑杆值变化（验证连续拖动）。
    Progress(f32),
    /// 文本输入变化（验证软键盘/IME）。
    Input(String),
}

impl Probe {
    /// 新建探针状态。
    pub fn new() -> Self {
        Self::default()
    }

    /// 处理消息。
    pub fn update(&mut self, message: Message) {
        match message {
            Message::Increment => self.count += 1,
            Message::Decrement => self.count -= 1,
            Message::Progress(value) => self.progress = value,
            Message::Input(value) => self.input = value,
        }
    }

    /// 构建视图。
    pub fn view(&self) -> Element<'_, Message, Theme, Renderer> {
        let counter = row![
            button("+1").on_press(Message::Increment),
            button("-1").on_press(Message::Decrement),
        ]
        .spacing(12);

        container(
            column![
                text("Lumino Android 探针").size(26),
                text("中文 / English 混排：黑乐谱编辑器渲染链路自检").size(15),
                text!("计数 = {}", self.count).size(20),
                counter,
                text!("滑杆 = {:.2}", self.progress).size(15),
                slider(0.0..=1.0, self.progress, Message::Progress).step(0.01_f32),
                text_input("输入点什么（验证软键盘）", &self.input).on_input(Message::Input),
                text!("已输入：{}", self.input).size(15),
            ]
            .align_x(Alignment::Center)
            .spacing(14),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center(Length::Fill)
        .padding(24)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter_messages() {
        let mut probe = Probe::new();
        probe.update(Message::Increment);
        probe.update(Message::Increment);
        probe.update(Message::Decrement);
        assert_eq!(probe.count, 1);
    }

    #[test]
    fn test_progress_and_input() {
        let mut probe = Probe::new();
        probe.update(Message::Progress(0.75));
        probe.update(Message::Input("中文".to_string()));
        assert!((probe.progress - 0.75).abs() < f32::EPSILON);
        assert_eq!(probe.input, "中文");
    }
}
