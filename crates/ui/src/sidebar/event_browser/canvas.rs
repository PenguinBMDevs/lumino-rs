//! 事件浏览器 Canvas 视图主体。
//!
//! 负责将事件浏览器状态渲染为 iced Canvas，处理树/表格/分页/编辑弹窗交互。
//! 绘制细节拆分为 `draw`，命中测试拆分为 `hit_test`，弹窗绘制拆分为 `popup`。

use iced_core::mouse::{Button, Event as MouseEvent, ScrollDelta};
use iced_core::{Font, Length, Point, Rectangle, Size};
use iced_widget::canvas::{self, Frame, Geometry, Program};
use lumino_extras::i18n::MainTranslations;

use crate::editor::grid::theme::ThemeExt;
use crate::sidebar::event_browser::detail::{self, EventBrowserData, EventTableRow};
use crate::sidebar::event_browser::edit::PopupState;
use crate::sidebar::event_browser::state::{ArchiveKey, EventBrowserState, SelectedItem};
use crate::sidebar::event_browser::table::{EVENT_PAGE_SIZE, total_pages};
use crate::sidebar::event_browser::tree::{TreeItem, collect_tree_items};
use crate::{Element, Message, Renderer, Theme};

mod draw;
pub(crate) mod draw_parts;
mod hit_test;
mod interaction;
mod popup;

/// 行高（像素）
pub const ROW_HEIGHT: f32 = 18.0;
/// 表头高度（像素）
pub const HEADER_HEIGHT: f32 = 20.0;
/// 字号
pub const FONT_SIZE: f32 = 11.0;
/// 树缩进（每层级像素）
pub const TREE_INDENT: f32 = 14.0;
/// 树/表格分隔条宽度
pub const SPLITTER_WIDTH: f32 = 6.0;
/// 树最小宽度
pub const MIN_TREE_WIDTH: f32 = 100.0;
/// 树最大宽度
pub const MAX_TREE_WIDTH: f32 = 200.0;
/// 最小列宽
pub const MIN_COL_WIDTH: f32 = 20.0;
/// 列分隔线命中半径
pub const DIVIDER_HIT_WIDTH: f32 = 4.0;
/// 分页器高度
pub const PAGER_HEIGHT: f32 = 28.0;
/// 分页器按钮宽度
pub const PAGER_BUTTON_WIDTH: f32 = 28.0;

/// Canvas 持久状态（由 iced 管理，跨帧保留）
#[derive(Debug)]
pub struct CanvasState {
    /// 当前各列宽度
    pub column_widths: Vec<f32>,
    /// 正在拖拽的列分隔线索引
    pub dragging_divider: Option<usize>,
    /// 拖拽开始时的鼠标 x
    pub drag_start_x: f32,
    /// 拖拽开始时的列宽快照
    pub drag_start_widths: Vec<f32>,
    /// 正在拖拽树/表格分隔条
    pub splitter_dragging: bool,
    /// 拖拽开始时的鼠标 x
    pub splitter_start_x: f32,
    /// 拖拽开始时的树宽度
    pub splitter_start_ratio: f32,
    /// 树宽度（绝对值，不随面板宽度变化，仅由分隔条拖拽改变）
    pub tree_width: f32,
    /// 垂直滚动偏移
    pub scroll_y: f32,
    /// 视口高度
    pub viewport_height: f32,
    /// 树项行高（可调整）
    pub tree_row_height: f32,
    /// 正在拖拽调整树项行高
    pub tree_row_resizing: bool,
    /// 树项行高拖拽开始时的鼠标 Y
    pub tree_row_resize_start_y: f32,
    /// 树项行高拖拽开始时的行高快照
    pub tree_row_resize_start_height: f32,
    /// 弹窗状态
    pub popup: Option<PopupState>,
    /// 右键上下文菜单（tick, x, y）
    pub context_menu: Option<(u32, Point)>,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            column_widths: Vec::new(),
            dragging_divider: None,
            drag_start_x: 0.0,
            drag_start_widths: Vec::new(),
            splitter_dragging: false,
            splitter_start_x: 0.0,
            splitter_start_ratio: 0.0,
            tree_width: MIN_TREE_WIDTH,
            scroll_y: 0.0,
            viewport_height: 0.0,
            tree_row_height: ROW_HEIGHT,
            tree_row_resizing: false,
            tree_row_resize_start_y: 0.0,
            tree_row_resize_start_height: ROW_HEIGHT,
            popup: None,
            context_menu: None,
        }
    }
}

/// 事件浏览器 Canvas
pub struct EventBrowserCanvas<'a> {
    /// 事件浏览器状态（来自 Sidebar，只读）
    pub state: &'a EventBrowserState,
    /// 渲染数据（只读引用）
    pub data: EventBrowserData<'a>,
    /// 多语言翻译
    pub t: &'static MainTranslations,
    /// 用于 Canvas 文本绘制的字体
    pub font: Font,
}

impl<'a> EventBrowserCanvas<'a> {
    /// 创建事件浏览器 Canvas
    pub fn new(
        state: &'a EventBrowserState,
        data: EventBrowserData<'a>,
        t: &'static MainTranslations,
        program_font_name: &str,
    ) -> Self {
        // Font::with_name 需要 &'static str，这里使用 Box::leak 转换运行时的字体名称。
        // 与 host/font.rs 中的 FONT_NAME_CACHE 思路一致。
        // 字体名称字符串通常很小（~16字节），且仅在用户修改设置时变化，泄漏量可忽略。
        let font = if program_font_name.is_empty() {
            Font::default()
        } else {
            let leaked: &'static str = Box::leak(program_font_name.to_string().into_boxed_str());
            Font::with_name(leaked)
        };
        Self {
            state,
            data,
            t,
            font,
        }
    }

    /// 根据程序字体名称创建 iced Font。
    /// 空名称时回退到默认字体。
    pub fn font(&self) -> Font {
        self.font
    }

    /// 树宽度 = 存储的绝对值，受 MIN_TREE_WIDTH / MAX_TREE_WIDTH 约束
    fn tree_width(&self, bounds: Rectangle, state: &CanvasState) -> f32 {
        let _ = bounds;
        state.tree_width.clamp(MIN_TREE_WIDTH, MAX_TREE_WIDTH)
    }

    /// 当前页的行切片（不可变版本，避免修改 state）
    fn page_slice<'b>(&self, rows: &'b [EventTableRow]) -> (usize, &'b [EventTableRow]) {
        let total_pages = total_pages(rows.len());
        let page = self.state.event_page.min(total_pages - 1);
        let start = page * EVENT_PAGE_SIZE;
        let end = (start + EVENT_PAGE_SIZE).min(rows.len());
        (page, &rows[start..end])
    }

    /// 收集展开状态下可见的树项
    fn visible_tree_items(&self) -> Vec<TreeItem> {
        let all = collect_tree_items(self.data.tracks, self.data.automation_lanes, self.t);
        let mut out = Vec::with_capacity(all.len());
        let mut hidden = 0usize;
        for item in &all {
            if hidden > 0 {
                hidden -= 1;
                continue;
            }
            let expandable = match item {
                TreeItem::Root { key, .. } => !self.state.expanded_keys.contains(key),
                TreeItem::Track { id, .. } => {
                    !self.state.expanded_keys.contains(&ArchiveKey::Track(*id))
                }
                TreeItem::Leaf { .. } => false,
            };
            // 被折叠节点的子项需要跳过：由调用方通过 collect 时记录数量处理。
            // 简化实现：Root/Track 折叠时隐藏其直接子项（假定子项紧跟其后）。
            if expandable {
                let children = count_children(item, &all);
                hidden = children;
            }
            out.push(item.clone());
        }
        out
    }
}

/// 计算树项的直接子项数量（简化：按深度变化推断）
fn count_children(item: &TreeItem, all: &[TreeItem]) -> usize {
    let index = all.iter().position(|i| i == item).unwrap_or(0);
    let depth = tree_depth(item);
    let mut count = 0;
    for next in all.iter().skip(index + 1) {
        let next_depth = tree_depth(next);
        if next_depth <= depth {
            break;
        }
        count += 1;
    }
    count
}

fn tree_depth(item: &TreeItem) -> u8 {
    match item {
        TreeItem::Root { .. } => 0,
        TreeItem::Leaf { depth, .. } | TreeItem::Track { depth, .. } => *depth,
    }
}

impl<'a> Program<Message, Theme, Renderer> for EventBrowserCanvas<'a> {
    type State = CanvasState;

    fn update(
        &self,
        state: &mut CanvasState,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: iced_core::mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        state.viewport_height = bounds.height;
        let pos = cursor.position()?;
        let local = Point::new(pos.x - bounds.x, pos.y - bounds.y);

        match event {
            // ── 键盘事件 ──
            canvas::Event::Keyboard(key_event) => self.handle_keyboard(state, key_event),
            // ── 鼠标滚轮 ──
            canvas::Event::Mouse(MouseEvent::WheelScrolled {
                delta: ScrollDelta::Lines { y, .. },
            }) => {
                // 计算最大滚动值，避免无内容可滚动时仍可滚动
                let tree_items = self.visible_tree_items();
                let tree_content_h = HEADER_HEIGHT + tree_items.len() as f32 * ROW_HEIGHT;
                let rows = detail::collect_rows(
                    self.state
                        .selected_item
                        .as_ref()
                        .unwrap_or(&SelectedItem::TimeSig),
                    &self.data,
                    self.t,
                );
                let (_, page_rows) = self.page_slice(&rows);
                let table_content_h = HEADER_HEIGHT + page_rows.len() as f32 * ROW_HEIGHT;
                let content_h = tree_content_h.max(table_content_h);
                let max_scroll = (content_h - state.viewport_height).max(0.0);

                state.scroll_y = (state.scroll_y + y * ROW_HEIGHT * 3.0).clamp(0.0, max_scroll);
                Some(canvas::Action::request_redraw())
            }
            // ── 鼠标按下 ──
            canvas::Event::Mouse(MouseEvent::ButtonPressed(Button::Left)) => {
                self.handle_left_press(state, bounds, local)
            }
            canvas::Event::Mouse(MouseEvent::ButtonPressed(Button::Right)) => {
                self.handle_right_press(state, bounds, local)
            }
            canvas::Event::Mouse(MouseEvent::ButtonReleased(Button::Left)) => {
                if state.dragging_divider.is_some()
                    || state.splitter_dragging
                    || state.tree_row_resizing
                {
                    state.dragging_divider = None;
                    state.splitter_dragging = false;
                    state.tree_row_resizing = false;
                    return Some(canvas::Action::capture());
                }
                None
            }
            // ── 鼠标移动 ──
            canvas::Event::Mouse(MouseEvent::CursorMoved { .. }) => {
                if let Some(idx) = state.dragging_divider {
                    let delta = local.x - state.drag_start_x;
                    let new_width = (state.drag_start_widths[idx] + delta).max(MIN_COL_WIDTH);
                    state.column_widths[idx] = new_width;
                    return Some(canvas::Action::capture());
                }
                if state.splitter_dragging {
                    let delta = local.x - state.splitter_start_x;
                    state.tree_width =
                        (state.splitter_start_ratio + delta).clamp(MIN_TREE_WIDTH, MAX_TREE_WIDTH);
                    return Some(canvas::Action::capture());
                }
                if state.tree_row_resizing {
                    let delta = local.y - state.tree_row_resize_start_y;
                    let new_height = (state.tree_row_resize_start_height + delta)
                        .clamp(ROW_HEIGHT * 0.5, ROW_HEIGHT * 3.0);
                    state.tree_row_height = new_height;
                    return Some(canvas::Action::capture());
                }
                None
            }
            _ => None,
        }
    }

    fn mouse_interaction(
        &self,
        state: &CanvasState,
        bounds: Rectangle,
        cursor: iced_core::mouse::Cursor,
    ) -> iced_core::mouse::Interaction {
        if state.dragging_divider.is_some() || state.splitter_dragging || state.tree_row_resizing {
            return iced_core::mouse::Interaction::ResizingHorizontally;
        }
        let Some(pos) = cursor.position() else {
            return iced_core::mouse::Interaction::default();
        };
        let local = Point::new(pos.x - bounds.x, pos.y - bounds.y);
        let tree_w = self.tree_width(bounds, state);
        // 树行高调整手柄（树区域底部表头线附近）
        if local.x < tree_w && (local.y - HEADER_HEIGHT).abs() <= 3.0 {
            return iced_core::mouse::Interaction::ResizingVertically;
        }
        // 树/表格分隔条
        if (local.x - tree_w - SPLITTER_WIDTH * 0.5).abs() <= SPLITTER_WIDTH * 0.5 {
            return iced_core::mouse::Interaction::ResizingHorizontally;
        }
        // 表格列分隔线
        if local.x > tree_w + SPLITTER_WIDTH
            && hit_test::hit_divider(local.x - tree_w - SPLITTER_WIDTH, &state.column_widths)
                .is_some()
        {
            return iced_core::mouse::Interaction::ResizingHorizontally;
        }
        iced_core::mouse::Interaction::default()
    }

    fn draw(
        &self,
        state: &CanvasState,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: iced_core::mouse::Cursor,
    ) -> Vec<Geometry<Renderer>> {
        let mut frame = Frame::new(renderer, bounds.size());

        let tree_items = self.visible_tree_items();
        let rows = detail::collect_rows(
            self.state
                .selected_item
                .as_ref()
                .unwrap_or(&SelectedItem::TimeSig),
            &self.data,
            self.t,
        );
        let (page, page_rows) = self.page_slice(&rows);
        let total_pages = total_pages(rows.len());

        draw::draw(
            &mut frame,
            theme,
            bounds,
            state,
            self,
            &tree_items,
            tree_items.len(),
            page_rows,
            page,
            total_pages,
        );

        // 右键上下文菜单
        if let Some((_, menu_pos)) = &state.context_menu {
            draw_context_menu(&mut frame, theme, *menu_pos, self.t, self.font());
        }

        vec![frame.into_geometry()]
    }
}

/// 渲染事件浏览器面板入口。
pub fn view_event_browser<'a>(
    state: &'a EventBrowserState,
    data: EventBrowserData<'a>,
    _context_menu_tick: Option<u32>,
    t: &'static MainTranslations,
    program_font_name: &str,
) -> Element<'a> {
    let canvas = EventBrowserCanvas::new(state, data, t, program_font_name);
    iced_widget::canvas::Canvas::new(canvas)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// 绘制右键上下文菜单
fn draw_context_menu(
    frame: &mut Frame<Renderer>,
    theme: &Theme,
    pos: Point,
    t: &MainTranslations,
    font: Font,
) {
    use iced_core::Color;
    let palette = theme.extended_palette();
    let is_light = theme.is_light();
    let bg = if is_light {
        Color::from_rgb(0.98, 0.98, 0.98)
    } else {
        palette.background.base.color
    };
    let fg = if is_light {
        Color::from_rgb(0.1, 0.1, 0.1)
    } else {
        palette.background.base.text
    };
    let border = if is_light {
        Color::from_rgb(0.7, 0.7, 0.7)
    } else {
        palette.background.strong.color
    };

    let item_h = 22.0;
    let width = 140.0;
    let height = 3.0 * item_h;
    let menu_rect = Rectangle::new(pos, Size::new(width, height));

    frame.fill_rectangle(menu_rect.position(), menu_rect.size(), bg);
    let mut path = iced_widget::canvas::path::Builder::new();
    let p = menu_rect.position();
    let s = menu_rect.size();
    path.move_to(p);
    path.line_to(Point::new(p.x + s.width, p.y));
    path.line_to(Point::new(p.x + s.width, p.y + s.height));
    path.line_to(Point::new(p.x, p.y + s.height));
    path.close();
    frame.stroke(
        &path.build(),
        iced_widget::canvas::Stroke::default()
            .with_color(border)
            .with_width(1.0),
    );

    let labels = [t.eb_insert_above, t.eb_insert_below, t.eb_delete];
    for (i, label) in labels.iter().enumerate() {
        let y = pos.y + i as f32 * item_h;
        frame.fill_text(iced_widget::canvas::Text {
            content: (*label).to_string(),
            position: Point::new(pos.x + 8.0, y + item_h * 0.5),
            color: fg,
            size: iced_core::Pixels(FONT_SIZE),
            line_height: iced_core::text::LineHeight::Absolute(iced_core::Pixels(FONT_SIZE + 2.0)),
            font,
            max_width: width - 16.0,
            align_x: iced_core::alignment::Horizontal::Left.into(),
            align_y: iced_core::alignment::Vertical::Center,
            shaping: iced_widget::text::Shaping::Basic,
        });
    }
}
