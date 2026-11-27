//! 吸附规格、工具栏下拉与粗细面板。

use super::*;

impl Toolbar {
    pub(super) fn update_snap(&mut self, event: Event) {
        match event {
            Event::SnapSpecChanged(spec) => {
                self.snap_spec = spec;
                self.snap_picker_open = false;
                tracing::debug!("工具栏: 吸附规格变更为 {:?}", spec);
            }
            Event::ToggleSnapPicker => {
                self.snap_picker_open = !self.snap_picker_open;
            }
            Event::CloseSnapPicker => {
                self.snap_picker_open = false;
            }
            Event::PrecisionChanged(precision) => {
                // 旧事件兼容：仅用于尚未迁移的调用点。
                self.snap_spec = SnapSpec::from_legacy(precision, None);
                tracing::debug!("工具栏: 旧精度事件 {:?} → {:?}", precision, self.snap_spec);
            }
            _ => {}
        }
    }

    pub(super) fn update_dropdowns(&mut self, event: Event) {
        match event {
            Event::ToggleOverflowMenu => {
                self.toggle_popover(Popover::OverflowMenu);
                // 与绘制工具面板互斥：打开溢出菜单时关闭工具面板
                self.tool_panel_open = false;
                tracing::debug!(
                    "工具栏: 溢出菜单 {}",
                    if self.is_popover_open(Popover::OverflowMenu) {
                        "打开"
                    } else {
                        "关闭"
                    }
                );
            }
            Event::CloseOverflowMenu => {
                self.close_popover(Popover::OverflowMenu);
                tracing::debug!("工具栏: 关闭溢出菜单");
            }
            Event::ToggleBrushDropdown => {
                self.toggle_popover(Popover::BrushDropdown);
                // 与其他面板互斥
                self.close_popover(Popover::OverflowMenu);
                self.tool_panel_open = false;
                tracing::debug!(
                    "工具栏: 画刷工具下拉 {}",
                    if self.is_popover_open(Popover::BrushDropdown) {
                        "打开"
                    } else {
                        "关闭"
                    }
                );
            }
            Event::CloseBrushDropdown => {
                self.close_popover(Popover::BrushDropdown);
                tracing::debug!("工具栏: 关闭画刷工具下拉");
            }
            Event::ToggleShapeDropdown => {
                self.toggle_popover(Popover::ShapeDropdown);
                // 与其他面板互斥：打开形状工具下拉时关闭其余浮层
                self.close_popover(Popover::OverflowMenu);
                self.tool_panel_open = false;
                self.close_popover(Popover::BrushDropdown);
                tracing::debug!(
                    "工具栏: 形状工具下拉 {}",
                    if self.is_popover_open(Popover::ShapeDropdown) {
                        "打开"
                    } else {
                        "关闭"
                    }
                );
            }
            Event::CloseShapeDropdown => {
                self.close_popover(Popover::ShapeDropdown);
                tracing::debug!("工具栏: 关闭形状工具下拉");
            }
            Event::ShapeTypeSelected(shape) => {
                // 切换当前图形类型（矩形/圆形/三角形）并持久保存到状态变量；
                // 下拉由 `dismiss_popovers` 在收到本事件时自动关闭（视为一次选择完成）。
                self.current_shape = shape;
                tracing::debug!("工具栏: 形状类型切换为 {:?}", shape);
            }
            _ => {}
        }
    }

    pub(super) fn update_thickness(&mut self, event: Event) {
        match event {
            Event::DrawThicknessChanged(thickness) => {
                // **全局**绘制粗细（唯一权威在 Toolbar::brush.thickness）：
                // 曲线/形状描边 + 画刷笔画共用；clamp 由 `set_thickness` 统一执行
                // （量程与粗细面板滑杆同源，见 `thickness_dropdown.rs`）。
                // 注意**不收面板**：本事件来自面板内滑杆（守卫已显式放行）。
                self.brush.set_thickness(thickness);
                tracing::debug!("工具栏: 全局绘制粗细变更为 {} 层", self.brush.thickness);
            }
            Event::ToggleThicknessPanel => {
                self.toggle_popover(Popover::ThicknessPanel);
                if self.is_popover_open(Popover::ThicknessPanel) {
                    // 与其余**窗口内**面板互斥（同一时刻至多一块；分音符填充面板的
                    // 状态在 `Root::state`，由 ToolbarHandler 的"外部动作即收起"统一处理）。
                    // 注意**不动** `tool_panel_open`：本面板锚定在悬浮条自己的按钮上，
                    // 关掉悬浮条等于把锚点一起拆了。
                    self.close_popover(Popover::BrushDropdown);
                    self.close_popover(Popover::ShapeDropdown);
                }
                tracing::debug!(
                    "工具栏: 粗细面板 {}",
                    if self.is_popover_open(Popover::ThicknessPanel) {
                        "打开"
                    } else {
                        "关闭"
                    }
                );
            }
            Event::CloseThicknessPanel => {
                self.close_popover(Popover::ThicknessPanel);
                self.close_popover(Popover::TextParams);
                tracing::debug!("工具栏: 关闭粗细面板");
            }
            _ => {}
        }
    }
}
