//! Toast 通知系统
//!
//! 提供 INFO/WARNING/ERROR/SUCCESS 四种级别的临时通知。
//!
//! ## 触发
//! 调用方直接调用 `ToastManager::push(level, message)` 添加 Toast，
//! 不通过 Message 系统路由，避免改动 lumino-message crate 的泛型签名。
//!
//! ## 渲染
//! `Root::view_main` 在主视图渲染完成后，通过 `ToastManager::view` 获取叠加层，
//! 使用 `iced_widget::Stack` 叠加在右下角。
//!
//! ## 过期清理
//! 每帧 `AnimationTick` 触发 `ToastManager::cleanup_expired`，移除过期 Toast。
//! 过期 Toast 不显示，也不占用渲染资源。

use std::time::{Duration, Instant};

use iced_core::{Alignment, Length, Padding};
use iced_widget::{Column, container, row, text};
use lumino_ui_core::{Element, Message, Theme};

/// Toast 级别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    /// 信息提示（蓝色）
    Info,
    /// 警告（橙色，常用于编辑拦截）
    Warning,
    /// 错误（红色）
    Error,
    /// 成功（绿色）
    Success,
}

impl ToastLevel {
    /// 图标字符（Unicode）
    fn icon(self) -> &'static str {
        match self {
            ToastLevel::Info => "ℹ",
            ToastLevel::Warning => "⚠",
            ToastLevel::Error => "✗",
            ToastLevel::Success => "✓",
        }
    }

    /// 默认显示时长
    fn default_duration(self) -> Duration {
        match self {
            ToastLevel::Info => Duration::from_millis(2500),
            ToastLevel::Warning => Duration::from_millis(3500),
            ToastLevel::Error => Duration::from_millis(5000),
            ToastLevel::Success => Duration::from_millis(2500),
        }
    }

    /// 背景色（基于 iced theme palette）
    fn background_color(self, theme: &Theme) -> iced_core::Color {
        let palette = theme.extended_palette();
        match self {
            ToastLevel::Info => palette.primary.strong.color,
            ToastLevel::Warning => iced_core::Color::from_rgb(0.95, 0.62, 0.17),
            ToastLevel::Error => palette.danger.base.color,
            ToastLevel::Success => palette.success.base.color,
        }
    }

    /// 文字颜色
    fn text_color(self) -> iced_core::Color {
        iced_core::Color::WHITE
    }
}

/// 单条 Toast
#[derive(Debug, Clone)]
pub struct Toast {
    pub id: u64,
    pub level: ToastLevel,
    pub message: String,
    pub created_at: Instant,
    pub duration: Duration,
}

impl Toast {
    /// 是否已过期
    pub fn is_expired(&self, now: Instant) -> bool {
        now.duration_since(self.created_at) >= self.duration
    }

    /// 剩余时长
    pub fn remaining(&self, now: Instant) -> Duration {
        self.duration
            .saturating_sub(now.duration_since(self.created_at))
    }
}

/// Toast 管理器
///
/// 由 `Root` 持有，负责 Toast 的添加、清理、渲染。
/// 不通过 Message 系统，调用方直接持有 `&mut ToastManager` 操作。
#[derive(Debug)]
pub struct ToastManager {
    toasts: Vec<Toast>,
    next_id: u64,
    /// 最大同时显示 Toast 数量（超出时移除最早的）
    max_visible: usize,
}

impl Default for ToastManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ToastManager {
    /// 创建 Toast 管理器
    pub fn new() -> Self {
        Self {
            toasts: Vec::new(),
            next_id: 1,
            max_visible: 4,
        }
    }

    /// 添加 Toast（使用级别默认时长），返回 Toast id
    pub fn push(&mut self, level: ToastLevel, message: impl Into<String>) -> u64 {
        self.push_with_duration(level, message, level.default_duration())
    }

    /// 添加 Toast（自定义时长），返回 Toast id
    pub fn push_with_duration(
        &mut self,
        level: ToastLevel,
        message: impl Into<String>,
        duration: Duration,
    ) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let toast = Toast {
            id,
            level,
            message: message.into(),
            created_at: Instant::now(),
            duration,
        };
        self.toasts.push(toast);
        // 超出最大显示数量则移除最早的
        while self.toasts.len() > self.max_visible {
            self.toasts.remove(0);
        }
        tracing::debug!(
            "Toast: 添加 (id={}, level={:?}, 剩余={})",
            id,
            level,
            self.toasts.len()
        );
        id
    }

    /// 主动移除指定 id 的 Toast
    pub fn dismiss(&mut self, id: u64) {
        if let Some(pos) = self.toasts.iter().position(|t| t.id == id) {
            self.toasts.remove(pos);
        }
    }

    /// 清理过期 Toast，返回移除数量
    ///
    /// 应在每帧 `AnimationTick` 中调用。
    pub fn cleanup_expired(&mut self, now: Instant) -> usize {
        let before = self.toasts.len();
        self.toasts.retain(|t| !t.is_expired(now));
        let removed = before - self.toasts.len();
        if removed > 0 {
            tracing::debug!("Toast: 清理过期 {} 条，剩余 {}", removed, self.toasts.len());
        }
        removed
    }

    /// 当前活跃 Toast 数量
    pub fn len(&self) -> usize {
        self.toasts.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.toasts.is_empty()
    }

    /// 获取活跃 Toast 切片（按添加顺序）
    pub fn toasts(&self) -> &[Toast] {
        &self.toasts
    }

    /// 渲染 Toast 叠加层
    ///
    /// 返回 `None` 表示当前无 Toast 需要渲染。
    /// 返回 `Some(element)` 时，调用方应使用 `Stack::push` 叠加到主视图上。
    pub fn view(&self, theme: &Theme) -> Option<Element<'_>> {
        if self.toasts.is_empty() {
            return None;
        }

        let mut col: Column<'_, Message, Theme, lumino_ui_core::Renderer> = Column::new()
            .spacing(6)
            .align_x(Alignment::End)
            .max_width(420.0);

        for toast in &self.toasts {
            let bg = toast.level.background_color(theme);
            let fg = toast.level.text_color();
            let icon = toast.level.icon();
            let message = toast.message.clone();

            let card = container(
                row![
                    text(icon).color(fg).size(16),
                    text(message).color(fg).size(14)
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding(Padding::new(10.0))
            .style(move |_theme: &Theme| container::Style {
                background: Some(iced_core::Background::Color(bg)),
                border: iced_core::Border {
                    radius: 6.0.into(),
                    width: 0.0,
                    color: iced_core::Color {
                        a: 0.2,
                        ..iced_core::Color::BLACK
                    },
                },
                ..Default::default()
            });

            col = col.push(card);
        }

        let overlay = container(col)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::End)
            .align_y(Alignment::End)
            .padding(Padding {
                top: 0.0,
                bottom: 36.0, // 避开状态栏
                left: 0.0,
                right: 16.0,
            });

        Some(overlay.into())
    }
}

/// Toast 内部使用的消息类型（当前无交互，预留扩展）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastMessage {
    /// 用户点击关闭按钮
    Dismiss(u64),
}

impl From<ToastMessage> for lumino_ui_core::Message {
    fn from(_msg: ToastMessage) -> Self {
        // 当前 Toast 不支持交互，转 Null 占位
        lumino_ui_core::Message::Null
    }
}

#[cfg(test)]
mod tests;
