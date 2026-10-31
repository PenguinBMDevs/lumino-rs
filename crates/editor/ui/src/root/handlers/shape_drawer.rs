//! 形状抽屉库/变换/插入动作（DRAW-033B2）

use crate::host::{DialogResult, ShapeInsertOptions};
use crate::message::ShapeDrawerAction;
use crate::root::Root;
use crate::state::root_state::ShapeDrawerState;
use lumino_core::shape::{ShapeDrawing, ShapeLibrary, delete_drawing, save_drawing};

impl Root {
    /// 处理形状抽屉动作；文件 I/O 与插入请求在此收口，Canvas 只做画板。
    pub(crate) fn handle_shape_drawer_action(&mut self, action: ShapeDrawerAction) {
        let state = &mut self.state.shape_drawer;
        match action {
            ShapeDrawerAction::SetName(name) => {
                state.name_input = name;
            }
            ShapeDrawerAction::Select(id) => select_drawing(state, &id),
            ShapeDrawerAction::New => {
                *state.doc.borrow_mut() = Default::default();
                state.selected = None;
                state.name_input = "New Shape".to_string();
                state.status = None;
            }
            ShapeDrawerAction::Save | ShapeDrawerAction::Rename => save_current(state),
            ShapeDrawerAction::Delete => delete_current(state),
            ShapeDrawerAction::Copy => {
                state.clipboard = state.doc.borrow().strokes.clone();
                state.status = Some(format!("已复制 {} 条 stroke", state.clipboard.len()));
            }
            ShapeDrawerAction::Paste => {
                let clipboard = state.clipboard.clone();
                if state.doc.borrow_mut().paste_strokes(&clipboard) {
                    state.status = Some("已粘贴".to_string());
                } else {
                    state.status = Some("剪贴板为空".to_string());
                }
            }
            ShapeDrawerAction::FlipHorizontal => {
                if state.doc.borrow_mut().flip_horizontal() {
                    state.status = Some("已水平翻转".to_string());
                }
            }
            ShapeDrawerAction::FlipVertical => {
                if state.doc.borrow_mut().flip_vertical() {
                    state.status = Some("已垂直翻转".to_string());
                }
            }
            ShapeDrawerAction::Rotate90 => {
                if state.doc.borrow_mut().rotate90() {
                    state.status = Some("已旋转 90°".to_string());
                }
            }
            ShapeDrawerAction::Insert => match current_drawing(state) {
                Ok(drawing) => match parse_insert_options(state) {
                    Ok(options) => {
                        self.state.dialog_result = Some(DialogResult::InsertShapeDrawing {
                            drawing: Box::new(drawing),
                            options,
                        });
                        state.status = Some("正在插入当前工程".to_string());
                    }
                    Err(error) => state.status = Some(error),
                },
                Err(error) => state.status = Some(error),
            },
            ShapeDrawerAction::PanelDragStart => {
                state.panel_dragging = true;
            }
            ShapeDrawerAction::PanelDragEnd => {
                state.panel_dragging = false;
            }
            ShapeDrawerAction::SetPanelWidth(width) => {
                if state.panel_dragging && width.is_finite() {
                    state.panel_width = width.clamp(180.0, 520.0);
                }
            }
            ShapeDrawerAction::ToggleAreaMode => {
                let enabled = !state.doc.borrow().area_mode;
                if state.doc.borrow_mut().set_area_mode(enabled) {
                    state.status = Some(if enabled {
                        "区域着色：点击闭合图形内部".to_string()
                    } else {
                        "已退出区域着色".to_string()
                    });
                } else {
                    state.status = Some("多笔画/开放图形暂不支持区域着色".to_string());
                }
            }
            ShapeDrawerAction::SetAreaColour(colour) => {
                if state.doc.borrow_mut().set_area_colour(colour) {
                    state.status = Some(format!("区域色：{colour}"));
                }
            }
            ShapeDrawerAction::ClearAreas => {
                if state.doc.borrow_mut().clear_areas() {
                    state.status = Some("已清空区域色".to_string());
                }
            }
            ShapeDrawerAction::SetInsertBeats(text) => state.insert_beats_text = text,
            ShapeDrawerAction::SetInsertKeys(text) => state.insert_keys_text = text,
            ShapeDrawerAction::ToggleInsertFit => {
                state.insert_fit_contain = !state.insert_fit_contain;
            }
            ShapeDrawerAction::SetScalePercent(text) => state.scale_percent_text = text,
            ShapeDrawerAction::ApplyScale => {
                let percent = state.scale_percent_text.trim().parse::<f32>().ok();
                match percent {
                    Some(percent) if (25.0..=200.0).contains(&percent) => {
                        if state.doc.borrow_mut().scale_selection(percent / 100.0) {
                            state.status = Some(format!("已缩放 {percent}%"));
                        } else {
                            state.status = Some("没有可缩放的 stroke".to_string());
                        }
                    }
                    _ => state.status = Some("缩放百分比需在 25–200".to_string()),
                }
            }
            ShapeDrawerAction::ToggleAnchorBend => {
                if state.doc.borrow_mut().toggle_selected_anchor_bend() {
                    state.status = Some("已切换锚点弯折".to_string());
                } else {
                    state.status = Some("请先在 Select 工具下选中曲线锚点".to_string());
                }
            }
        }
    }
}

fn select_drawing(state: &mut ShapeDrawerState, id: &str) {
    let Some(drawing) = state.library.find(id).cloned() else {
        state.status = Some(format!("找不到图形 {id}"));
        return;
    };
    state.doc.borrow_mut().load_drawing(&drawing);
    state.selected = Some(drawing.id.clone());
    state.name_input = drawing.name;
    state.status = None;
}

fn current_drawing(state: &ShapeDrawerState) -> Result<ShapeDrawing, String> {
    let name = state.name_input.trim();
    if name.is_empty() {
        return Err("形状名称不能为空".to_string());
    }
    let selected_user = state
        .selected
        .as_deref()
        .filter(|id| state.library.user.iter().any(|drawing| drawing.id == *id));
    let base = selected_user.unwrap_or(name);
    let keep_selected = selected_user.is_some();
    let id = unique_user_id(state, base, keep_selected);
    Ok(state.doc.borrow().to_drawing(id, name.to_string()))
}

/// 解析抽屉插入参数（与主窗 `insert_shape_drawing` 使用同一套校验）
fn parse_insert_options(state: &ShapeDrawerState) -> Result<ShapeInsertOptions, String> {
    let beats = state
        .insert_beats_text
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or_else(|| "插入宽度需为正数（拍）".to_string())?;
    let keys = state
        .insert_keys_text
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite() && (1.0..=128.0).contains(value))
        .ok_or_else(|| "插入高度需在 1–128 key".to_string())?;
    Ok(ShapeInsertOptions {
        beats,
        keys,
        fit_contain: state.insert_fit_contain,
    })
}

fn unique_user_id(state: &ShapeDrawerState, base: &str, keep_selected: bool) -> String {
    let taken = |id: &str| {
        state.library.builtin.iter().any(|drawing| drawing.id == id)
            || state.library.user.iter().any(|drawing| drawing.id == id)
    };
    if keep_selected
        && !state
            .library
            .builtin
            .iter()
            .any(|drawing| drawing.id == base)
    {
        return base.to_string();
    }
    if !taken(base) {
        return base.to_string();
    }
    for suffix in 2..=u32::MAX {
        let candidate = format!("{base}-{suffix}");
        if !taken(&candidate) {
            return candidate;
        }
    }
    format!("{base}-dup")
}

fn save_current(state: &mut ShapeDrawerState) {
    if state.shapes_dir.as_os_str().is_empty() {
        state.status = Some("形状库目录尚未注入".to_string());
        return;
    }
    let drawing = match current_drawing(state) {
        Ok(drawing) => drawing,
        Err(error) => {
            state.status = Some(error);
            return;
        }
    };
    match save_drawing(&state.shapes_dir, &drawing) {
        Ok(()) => {
            state.selected = Some(drawing.id.clone());
            state.name_input = drawing.name.clone();
            reload_library(state);
            state.status = Some(format!("已保存 {}", drawing.name));
        }
        Err(error) => state.status = Some(format!("保存失败：{error}")),
    }
}

fn delete_current(state: &mut ShapeDrawerState) {
    let Some(id) = state.selected.clone() else {
        state.status = Some("请先选择库内图形".to_string());
        return;
    };
    if state.shapes_dir.as_os_str().is_empty() {
        state.status = Some("形状库目录尚未注入".to_string());
        return;
    }
    match delete_drawing(&state.shapes_dir, &id) {
        Ok(true) => {
            reload_library(state);
            state.selected = None;
            *state.doc.borrow_mut() = Default::default();
            state.name_input = "New Shape".to_string();
            state.status = Some(format!("已删除 {id}"));
        }
        Ok(false) => state.status = Some(format!("图形 {id} 不存在")),
        Err(error) => state.status = Some(format!("删除失败：{error}")),
    }
}

fn reload_library(state: &mut ShapeDrawerState) {
    if state.shapes_dir.as_os_str().is_empty() {
        state.library = ShapeLibrary::with_builtins();
        state.warnings.clear();
        return;
    }
    let (library, warnings) = ShapeLibrary::load_dir(&state.shapes_dir);
    state.library = library;
    state.warnings = warnings;
}
