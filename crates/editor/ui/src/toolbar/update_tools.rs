//! 工具选择、绘制工具条（常驻浮层）与拖拽。

use super::*;

impl Toolbar {
    pub(super) fn update_tools(&mut self, event: Event) {
        match event {
            Event::ToolSelected(tool) => {
                self.current_tool = tool;
                // 切换工具即离开任何共存态：填充桶仅曲线/形状可共存，切到其它工具一律关闭
                self.fill_enabled = false;
                // 三点圆弧属于曲线工具的落点模式：任何显式工具切换都退出
                self.arc_mode = false;
                // 关闭附着于工具栏的下拉（画刷/形状），避免工具切换后残留；
                // 但**不关闭**音符绘制悬浮工具条——它是独立常驻浮层，与工具切换无关。
                self.close_popover(Popover::BrushDropdown);
                self.close_popover(Popover::ShapeDropdown);
            }
            Event::FillToggled(enabled) => {
                self.fill_enabled = enabled;
                if enabled {
                    // 填充与三点圆弧落点互斥：开启填充即退出圆弧模式
                    self.arc_mode = false;
                }
                tracing::debug!("工具栏: 颜料桶填充模式切换为 {}", enabled);
            }
            Event::ToggleToolPanel => {
                self.tool_panel_open = !self.tool_panel_open;
                // 与溢出菜单、画刷/形状下拉/粗细面板互斥：打开绘制工具条时关闭其余浮层
                self.close_popover(Popover::OverflowMenu);
                self.close_popover(Popover::BrushDropdown);
                self.close_popover(Popover::ShapeDropdown);
                // 关闭时一并收起粗细面板：它锚定在悬浮条的按钮上，悬浮条不在就没有锚点；
                // 留着"开"状态会让下次打开悬浮条时凭空弹出一块面板（用户早已看不见它）
                self.close_popover(Popover::ThicknessPanel);
                self.close_popover(Popover::TextParams);
                // 关闭时复位拖拽态与抓取点，避免残留的全窗口拖拽覆盖层拦截后续交互
                if !self.tool_panel_open {
                    self.tool_panel_dragging = false;
                    self.tool_panel_last_cursor = None;
                }
                tracing::debug!(
                    "工具栏: 音符绘制悬浮工具条 {}",
                    if self.tool_panel_open {
                        "打开"
                    } else {
                        "关闭"
                    }
                );
            }
            Event::CloseToolPanel => {
                self.tool_panel_open = false;
                self.tool_panel_dragging = false;
                self.tool_panel_last_cursor = None;
                // 悬浮条关闭即无从承载工具设置下拉 / 粗细面板，一并收起
                self.close_popover(Popover::BrushDropdown);
                self.close_popover(Popover::ShapeDropdown);
                self.close_popover(Popover::ThicknessPanel);
                self.close_popover(Popover::TextParams);
                tracing::debug!("工具栏: 关闭音符绘制悬浮工具条");
            }
            Event::ToolPanelItemSelected(item) => {
                // 普通点击：仅执行选择语义（切工具 / 切换填充共存态），保持面板打开。
                self.apply_tool_panel_item(item);
                self.close_popover(Popover::TextParams);
                // 选中后**保持面板打开**：悬浮工具条是常驻浮层，允许连续切换绘制工具
                // （与旧下拉"逐项选择即关闭"行为不同——那是一次性下拉的语义）。
                tracing::debug!("工具栏: 绘制工具条选择 {:?}", item);
            }
            Event::ToolPanelItemSettingsRequested(item) => self.open_tool_settings(item),
            Event::CloseTextParamsPanel => {
                self.close_popover(Popover::TextParams);
                tracing::debug!("工具栏: 关闭文字参数面板");
            }
            _ => {}
        }
    }

    pub(super) fn update_tool_panel_drag(&mut self, event: Event) {
        match event {
            Event::ToolPanelDragStarted => {
                self.tool_panel_dragging = true;
                self.tool_panel_last_cursor = None;
            }
            Event::ToolPanelDragEnded => {
                self.tool_panel_dragging = false;
                self.tool_panel_last_cursor = None;
                // 松手即尝试吸附回默认位（接近默认点才吸，见方法注释）
                self.snap_tool_panel_if_near_default();
            }
            Event::ToolPanelDragged(px, py) => {
                // px/py 为全窗口覆盖层给出的绝对光标位置；以增量方式跟随，
                // 使面板在光标离开面板/窗口范围时仍持续移动。
                if self.tool_panel_dragging {
                    match self.tool_panel_last_cursor {
                        None => self.tool_panel_last_cursor = Some((px, py)),
                        Some((lx, ly)) => {
                            let (dx, dy) = self.tool_panel_offset;
                            // 向右拖 → dx 增大；向上拖（py 减小）→ 距底内缩 dy 增大
                            self.tool_panel_offset = (
                                (dx + (px - lx)).clamp(-4000.0, 4000.0),
                                (dy - (py - ly)).clamp(0.0, 4000.0),
                            );
                            self.tool_panel_last_cursor = Some((px, py));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// 「已启用条目被再次点击」= 弹出该工具的设置面板。
    fn open_tool_settings(&mut self, item: ToolPanelItem) {
        // 视图层只在条目已启用时发本事件（见 `root/draw_toolbar.rs::tool_panel_item_press`），
        // 因此这里仍先套用一次选择语义（幂等，且保证填充态不被 toggle 掉）。
        self.apply_tool_panel_item(item);
        self.close_popover(Popover::TextParams);
        match item {
            ToolPanelItem::Brush => {
                // 画刷设置下拉（全局粗细只读显示 / 绘制行为入口）
                self.open_popover(Popover::BrushDropdown);
                self.close_popover(Popover::ShapeDropdown);
            }
            ToolPanelItem::Shape => {
                // 形状设置下拉（矩形 / 圆形 / 三角形），二者互斥
                self.open_popover(Popover::ShapeDropdown);
                self.close_popover(Popover::BrushDropdown);
            }
            ToolPanelItem::FillBucket => {
                // 颜料桶的设置 = 「分音符填充」面板；`apply_tool_panel_item` 对颜料桶
                // 是 toggle 语义，可能把刚点亮的填充关掉，这里显式复位为"开启"。
                // 关闭填充走面板内的「关闭填充」按钮（`FillToggled(false)`）。
                self.fill_enabled = true;
            }
            // 曲线 / 文字无独立设置（视图层不会为本事件选中它们）
            ToolPanelItem::Text => {
                self.open_popover(Popover::TextParams);
                self.close_popover(Popover::BrushDropdown);
                self.close_popover(Popover::ShapeDropdown);
                self.close_popover(Popover::ThicknessPanel);
            }
            _ => {}
        }
        tracing::debug!("工具栏: 绘制工具条再次点击 → 打开设置 {:?}", item);
    }

    /// 应用「绘制工具条条目」的**普通选择语义**（切换工具 / 切换填充共存态）。
    ///
    /// 抽为独立方法，供 `ToolPanelItemSelected`（普通点击）与
    /// `ToolPanelItemSettingsRequested`（已启用条目再次点击：选择 + 打开设置）复用，
    /// 避免两处选择逻辑漂移。
    fn apply_tool_panel_item(&mut self, item: ToolPanelItem) {
        // 除「圆弧」外，任何工具条目选择都退出三点圆弧模式
        self.arc_mode = item == ToolPanelItem::Arc;
        match item {
            ToolPanelItem::Mouse => {
                // 鼠标工具（图形选中）：独立工具，不与填充桶共存；
                // 仅用于点选已确认的绘制图形，不参与音符编辑。
                self.current_tool = Tool::ShapeSelect;
                self.fill_enabled = false;
            }
            ToolPanelItem::Curve => {
                // 曲线工具：独立基础工具，选中后关闭填充共存态
                // （填充由「填充桶」条目单独开启）
                self.current_tool = Tool::Curve;
                self.fill_enabled = false;
            }
            ToolPanelItem::Arc => {
                // 三点圆弧：复用曲线工具的画布与确认链路，仅落点语义不同
                self.current_tool = Tool::Curve;
                self.fill_enabled = false;
            }
            ToolPanelItem::FillBucket => {
                // 颜料桶随时可切换：仅对曲线/形状绘制的封闭图形生效，
                // 即使当前不在曲线工具也可开启，作用范围由编辑器侧控制。
                self.fill_enabled = !self.fill_enabled;
            }
            ToolPanelItem::Brush => {
                // 画刷仅可独立使用，不可与填充桶共存
                self.current_tool = Tool::Brush;
                self.fill_enabled = false;
            }
            ToolPanelItem::Shape => {
                // 形状工具：与曲线互斥（单一 base 工具），可与填充桶共存，
                // 选中形状时先关闭填充，再由「填充桶」条目按需开启
                self.current_tool = Tool::Shape;
                self.fill_enabled = false;
            }
            ToolPanelItem::Text => {
                // 文字工具：独立工具，不可与任何工具/填充桶共存
                self.current_tool = Tool::Text;
                self.fill_enabled = false;
            }
        }
    }

    /// 拖拽结束时的**自动吸附**：释放点接近默认位（水平居中、距底 44px）时吸回默认位。
    ///
    /// 判定为"每轴距离均 ≤ `TOOL_PANEL_SNAP_DISTANCE`"，即面板横向大致居中且纵向大致
    /// 落在默认高度带内才吸附；否则保留释放点（允许用户把工具条停到任意位置）。
    fn snap_tool_panel_if_near_default(&mut self) {
        let (dx, dy) = self.tool_panel_offset;
        let (default_dx, default_dy) = TOOL_PANEL_DEFAULT_OFFSET;
        if (dx - default_dx).abs() <= TOOL_PANEL_SNAP_DISTANCE
            && (dy - default_dy).abs() <= TOOL_PANEL_SNAP_DISTANCE
        {
            self.tool_panel_offset = TOOL_PANEL_DEFAULT_OFFSET;
            tracing::debug!(
                "工具栏: 悬浮工具条吸附回默认位 {:?}",
                TOOL_PANEL_DEFAULT_OFFSET
            );
        }
    }
}
