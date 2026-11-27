//! 主窗口 band 编排：根 Ui、底层纹理、六条面板带与状态栏。
//!
//! 本文件的调用顺序就是层级契约（DRAW-045 / DRAW-047）：底层纹理 → 第一屏 →
//! 走带与渲染器入口 → 面板带 → 覆盖层。新增 band 必须插进既有阶段之间，
//! 不允许在末尾追加，否则会盖住浮层或画布预览。

use egui::Panel;

use crate::icons::IconCache;
use crate::layout;
use crate::theme;

use super::channels::{
    arrangement_model, base_texture, first_screen_model, renderer_panel_model, video_timeline,
    window_rect,
};
use super::model::ChromeModel;
use super::{
    arrangement, dialog_layer, first_screen, menu, overlays, panel_frame, piano_roll, sidebar,
    statusbar, titlebar, toolbar,
};

/// 绘制主窗口全部 band 与覆盖层。
pub fn show_bands(ui: &mut egui::Ui, model: &ChromeModel, icons: &mut IconCache) {
    let mut root = prepare_root(ui);
    show_background_layers(&mut root, model, icons);
    if dialog_layer::show(&mut root, model, icons) {
        return;
    }
    show_panel_bands(&mut root, model, icons);
    overlays::show(ui, &mut root, model, icons);
}

/// 重建铺满视口的根 Ui，并装好主题、字体与零间距。
fn prepare_root(ui: &mut egui::Ui) -> egui::Ui {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());

    // `egui_kittest`（以及部分宿主）给到的 Ui 带 8px 外缩进；Panel 需要铺满视口，
    // 否则 band 会整体内缩、与 上一代 原位置错开。生产接线时根 Ui 本就是全屏，
    // 这里重建等价于无操作。
    let full = ui.ctx().viewport_rect();
    let mut root = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(full)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    root.set_clip_rect(full);
    root.spacing_mut().item_spacing = egui::Vec2::ZERO;
    root
}

/// 底层纹理、第一屏与渲染器入口层：都画在面板带之下。
fn show_background_layers(root: &mut egui::Ui, model: &ChromeModel, icons: &mut IconCache) {
    show_base_texture(root);
    show_first_screen(root, model, icons);
    show_arrangement(root);
    show_renderer_panel(root);
    show_video_timeline(root, icons);
}

/// 底层画布：整窗采样 `lumino-gfx` 的离屏纹理（取代 gfx → surface 的 blit）。
fn show_base_texture(root: &mut egui::Ui) {
    // 必须最先画，后面的 band / 面板都在它上面。
    if let Some(id) = base_texture(root.ctx()) {
        root.painter().image(
            id,
            root.max_rect(),
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }
}

/// 「无面板」默认第一屏：主区域没有任何视图时的品牌占位 / 无工程操作区。
///
/// 画在底层纹理之上、各 band 之下；按钮矩形登记为占用区，避免同一击
/// 又落到下层画布。
fn show_first_screen(root: &mut egui::Ui, model: &ChromeModel, icons: &mut IconCache) {
    let blocked = if let Some(first) = first_screen_model(root.ctx()) {
        let full = window_rect(root);
        let left = model
            .sidebar
            .as_ref()
            .map(|sidebar| sidebar.total_width())
            .unwrap_or(0.0);
        let right = model
            .right_sidebar
            .as_ref()
            .map(|sidebar| sidebar.bar_width())
            .unwrap_or(0.0);
        let titlebar_h = if model.titlebar.is_some() {
            layout::TITLEBAR_H
        } else {
            0.0
        };
        let top = titlebar_h
            + model
                .toolbar
                .as_ref()
                .map(|toolbar| toolbar.height.max(1.0))
                .unwrap_or(0.0);
        let area = egui::Rect::from_min_max(
            egui::pos2(full.left() + left, full.top() + top),
            egui::pos2(
                (full.right() - right).max(full.left() + left),
                (full.bottom() - layout::STATUSBAR_H).max(full.top() + top),
            ),
        );
        first_screen::show(root, area, icons, &first)
    } else {
        Vec::new()
    };

    // 本帧菜单占用区先清空：未画菜单时不能残留上一帧的矩形（会吞掉 上一代 点击）。
    menu::clear_blocked(root.ctx());
    if !blocked.is_empty() {
        menu::extend_blocked(root.ctx(), blocked);
    }
}

/// 走带标尺：`LUMINO_EGUI_ARRANGEMENT=0` 时回退 上一代。
fn show_arrangement(root: &mut egui::Ui) {
    if arrangement_enabled()
        && let Some(arrangement) = arrangement_model(root.ctx())
        && let Some(action) = arrangement::show(root, root.max_rect(), &arrangement)
    {
        arrangement::push_action(root.ctx(), action);
    }
}

/// 渲染器入口面板（轨道占位 / 16:9 预览 / 时间轴卡片 / 导出设置）。
fn show_renderer_panel(root: &mut egui::Ui) {
    if let Some(panel) = renderer_panel_model(root.ctx()) {
        crate::renderer_panel::show(root, &panel);
    }
}

/// 视频剪辑时间轴（渲染器入口面板内）。
fn show_video_timeline(root: &mut egui::Ui, icons: &mut IconCache) {
    if !video_timeline_enabled() {
        return;
    }
    let Some((rect, timeline)) = video_timeline(root.ctx()) else {
        return;
    };
    crate::video_clip_timeline::show_title_row(root, &timeline, icons);
    if let Some(bar) = timeline.scrollbar
        && let Some(action) = crate::video_timeline_scrollbar::show(
            root,
            bar,
            timeline.scroll_x,
            timeline.content_w,
            timeline.viewport_w,
            timeline.zoom,
        )
    {
        super::channels::push_video_timeline_action(root.ctx(), action);
    }
    crate::video_clip_timeline::show(root, rect, &timeline);
    // 时间轴由 egui 接管输入：整块占用，避免画布再处理一次。
    menu::extend_blocked(root.ctx(), [rect]);
    for action in crate::video_clip_timeline::interact(root, rect, &timeline) {
        super::channels::push_video_timeline_action(root.ctx(), action);
    }
}

/// 六条面板带与状态栏，顺序即生产布局。
fn show_panel_bands(root: &mut egui::Ui, model: &ChromeModel, icons: &mut IconCache) {
    show_titlebar_band(root, model, icons);
    show_toolbar_band(root, model, icons);
    show_left_sidebar_band(root, model, icons);
    show_right_sidebar_band(root, model, icons);
    show_piano_roll_band(root, model);
    show_velocity_band(root);
    show_statusbar_band(root, model);
}

fn show_titlebar_band(root: &mut egui::Ui, model: &ChromeModel, icons: &mut IconCache) {
    let Some(titlebar) = &model.titlebar else {
        return;
    };
    let focused = titlebar.focused;
    Panel::top("lumino-titlebar-band")
        .exact_size(layout::TITLEBAR_H)
        .resizable(false)
        .show_separator_line(false)
        .frame(panel_frame(if focused {
            theme::titlebar_bg()
        } else {
            theme::route_bar_bg()
        }))
        .show(root, |ui| titlebar::show_model(ui, titlebar, icons));
}

/// 工具栏 band：从左侧栏右边缘开始，铺满剩余宽度（生产 `row![sidebar,
/// column![toolbar, ...]]`），不覆盖左侧栏。顶部偏移跟随标题栏 band：
/// macOS 原生标题栏模式没有 30px 菜单栏行（上一代 主内容直接从工具栏开始）。
fn show_toolbar_band(root: &mut egui::Ui, model: &ChromeModel, icons: &mut IconCache) {
    let Some(toolbar) = &model.toolbar else {
        return;
    };
    let full = window_rect(root);
    let inset = toolbar.left_inset.max(0.0).min(full.width());
    let top = if model.titlebar.is_some() {
        full.top() + layout::TITLEBAR_H
    } else {
        full.top()
    };
    let band = egui::Rect::from_min_size(
        egui::pos2(full.left() + inset, top),
        egui::Vec2::new(full.width() - inset, toolbar.height.max(1.0)),
    );
    if band.width() > 0.0 {
        root.painter().rect_filled(band, 0, theme::panel_bg());
        let mut child = root.new_child(
            egui::UiBuilder::new()
                .max_rect(band)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        toolbar::show_model(&mut child, toolbar, icons);
        // 工具栏由 egui 独占：整块计入占用区，否则同一次点击会先被 上一代 的
        // 按钮处理（例如 Play 启动播放），再被 egui 处理（此时
        // `playing` 已变 true → 发 Pause），表现为「点播放先响一下又停」。
        // 底部 `TOOLBAR_RESIZE_H` 条仍留给 上一代（工具栏高度拖拽尚未迁移）。
        let blocked = egui::Rect::from_min_max(
            band.min,
            egui::pos2(
                band.right(),
                (band.bottom() - layout::TOOLBAR_RESIZE_H).max(band.top()),
            ),
        );
        if blocked.height() > 0.0 {
            menu::extend_blocked(root.ctx(), [blocked]);
        }
    }
}

/// 左侧栏 band：生产布局是 `column![titlebar, row![sidebar, content], statusbar]`，
/// 因此左侧栏从标题栏下沿开始、到状态栏上沿结束，宽度 = 48 + panel_width。
fn show_left_sidebar_band(root: &mut egui::Ui, model: &ChromeModel, icons: &mut IconCache) {
    let Some(sidebar) = &model.sidebar else {
        return;
    };
    let full = window_rect(root);
    let top = if model.titlebar.is_some() {
        full.top() + layout::TITLEBAR_H
    } else {
        full.top()
    };
    let bottom = full.bottom() - layout::STATUSBAR_H;
    let width = sidebar.total_width().max(0.0).min(full.width());
    let band = egui::Rect::from_min_max(
        egui::pos2(full.left(), top),
        egui::pos2(full.left() + width, bottom.max(top)),
    );
    if band.width() > 0.0 && band.height() > 0.0 {
        root.painter().rect_filled(band, 0, theme::panel_bg());
        let mut child = root.new_child(
            egui::UiBuilder::new()
                .max_rect(band)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        sidebar::show_model(&mut child, sidebar, icons);
    }
}

/// 右侧栏 band：生产布局是 `column![toolbar, row![editor, right_bar]]`，
/// 因此图标列从标题栏 + 工具栏下沿开始、到状态栏上沿结束。面板可见且内容
/// 已由 egui 接管时，band 扩到「面板宽 + 48」，覆盖 上一代 的同位置内容。
fn show_right_sidebar_band(root: &mut egui::Ui, model: &ChromeModel, icons: &mut IconCache) {
    let Some(right) = &model.right_sidebar else {
        super::right_panel::clear_panel_rect(root.ctx());
        return;
    };
    let full = window_rect(root);
    let top = (full.top() + right.top_inset).max(full.top());
    let bottom = (full.bottom() - layout::STATUSBAR_H).max(top);
    let content = right
        .content
        .as_ref()
        .filter(|_| right.panel_visible && bottom > top);
    let total = if content.is_some() {
        right.panel_width + right.bar_width()
    } else {
        right.bar_width()
    };
    let width = total.max(0.0).min(full.width());
    let band = egui::Rect::from_min_max(
        egui::pos2(full.right() - width, top),
        egui::pos2(full.right(), bottom),
    );
    if band.width() <= 0.0 || band.height() <= 0.0 {
        super::right_panel::clear_panel_rect(root.ctx());
        return;
    }
    if let Some(content) = content {
        // 手柄 6px（生产 `RESIZE_HANDLE_WIDTH`）由 egui 绘制并独占命中
        // （`right_panel::show_handle` 自己计入占用列表）；宽度跟随由宿主在
        // `CursorMoved` 里调 `update_resize_position`，egui 只发成对起止信号。
        let handle = egui::Rect::from_min_max(
            band.min,
            egui::pos2((band.left() + 6.0).min(band.right()), band.bottom()),
        );
        super::right_panel::show_handle(root, handle, right.resizing);
        let content_rect = egui::Rect::from_min_max(
            egui::pos2(handle.right(), band.top()),
            egui::pos2(band.right() - right.bar_width(), band.bottom()),
        );
        if content_rect.width() > 0.0 {
            super::right_panel::set_panel_rect(root.ctx(), content_rect);
            super::right_panel::show(root, content_rect, content, icons);
        }
    } else {
        super::right_panel::clear_panel_rect(root.ctx());
    }

    let bar = egui::Rect::from_min_max(
        egui::pos2(band.right() - right.bar_width(), band.top()),
        band.max,
    );
    let mut child = root.new_child(
        egui::UiBuilder::new()
            .max_rect(bar)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    sidebar::show_right_model(&mut child, right, icons);
}

/// 钢琴卷帘骨架 band：只画键盘列 / 标尺行 / 两条滚动条，
/// 中央网格区域保持透明（下层是 `lumino-gfx` 的 wgpu 音符与网格）。
fn show_piano_roll_band(root: &mut egui::Ui, model: &ChromeModel) {
    let Some(piano) = &model.piano_roll else {
        return;
    };
    let area = piano.area(root.max_rect());
    if area.width() > 0.0 && area.height() > 0.0 {
        let child = root.new_child(
            egui::UiBuilder::new()
                .max_rect(area)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        piano_roll::draw_chrome(&child, piano);
    }
}

/// 力度 / CC / Tempo 面板骨架：工具栏 + 网格 + 刻度标签。
/// 数据曲线由 gfx 画进底层纹理；本层不铺背景（否则盖住曲线）。
fn show_velocity_band(root: &mut egui::Ui) {
    let Some(panel) = super::channels::velocity_panel_model(root.ctx()) else {
        return;
    };
    let (action, blocked) = crate::velocity_panel::show(root, &panel);
    if let Some(action) = action {
        crate::velocity_panel::push_action(root.ctx(), action);
    }
    if !blocked.is_empty() {
        menu::extend_blocked(root.ctx(), blocked);
    }
}

fn show_statusbar_band(root: &mut egui::Ui, model: &ChromeModel) {
    Panel::bottom("lumino-statusbar-band")
        .exact_size(layout::STATUSBAR_H)
        .resizable(false)
        .show_separator_line(false)
        .frame(panel_frame(theme::toolbar_bg()))
        .show(root, |ui| statusbar::show_model(ui, &model.statusbar));
}

/// 走带视图是否已由 egui 接管（**默认开启**；`LUMINO_EGUI_ARRANGEMENT=0` 回退 上一代）。
///
/// 实测：egui 的泳道点击落点与指针一致，而 上一代 的 click canvas 存在
/// canvas 偏移量级（≈208 逻辑像素）的既有偏差；标尺/轨道列在画布侧不绘制内容。
/// 因此默认走 egui 属于修复，而不是等价切换。
fn arrangement_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("LUMINO_EGUI_ARRANGEMENT")
            .map(|v| v != "0")
            .unwrap_or(true)
    })
}

/// 时间轴是否由 egui 接管。
///
/// **默认开启**：上一代 侧的 `TimelineCanvas` 已删除（面板里只剩同尺寸占位），
/// 关闭开关只会得到空白区域；`LUMINO_EGUI_CLIP_TIMELINE=0` 仅用于对照排查。
fn video_timeline_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("LUMINO_EGUI_CLIP_TIMELINE")
            .map(|v| v != "0")
            .unwrap_or(true)
    })
}
