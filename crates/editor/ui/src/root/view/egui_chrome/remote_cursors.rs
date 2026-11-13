//! 协作远程光标：生产 `Editor::remote_cursors` → egui 叠加层模型。

use egui::Color32;
use lumino_ui_egui::remote_cursors::{RemoteCursorMark, RemoteCursorModel};

use crate::root::Root;

impl Root {
    /// 本帧远程光标（窗口逻辑坐标）。
    ///
    /// 旧版绘制口径：文档像素坐标 − 视图滚动 + 画布左上角偏移；颜色是协作者
    /// 下发的 `#RRGGBB` / `#RRGGBBAA` 字符串。
    pub(crate) fn remote_cursors_model(&self) -> RemoteCursorModel {
        let view = &self.editor.editor_state.view;
        let canvas = &self.editor.editor_state.canvas;
        let cursors = self
            .editor
            .remote_cursors
            .values()
            .map(|(pos, color, username)| RemoteCursorMark {
                pos: egui::pos2(
                    pos.x - view.scroll_x + canvas.offset_x,
                    pos.y - view.scroll_y + canvas.offset_y,
                ),
                color: parse_color(color).unwrap_or(Color32::WHITE),
                username: username.clone(),
            })
            .collect();
        RemoteCursorModel { cursors }
    }
}

/// 解析 `#RGB` / `#RRGGBB` / `#RRGGBBAA`（无 `#` 也接受）。
fn parse_color(text: &str) -> Option<Color32> {
    let hex = text.trim().trim_start_matches('#');
    let digits = |s: &str| u8::from_str_radix(s, 16).ok();
    match hex.len() {
        3 => {
            let r = digits(&hex[0..1])?;
            let g = digits(&hex[1..2])?;
            let b = digits(&hex[2..3])?;
            Some(Color32::from_rgb(r * 17, g * 17, b * 17))
        }
        6 => Some(Color32::from_rgb(
            digits(&hex[0..2])?,
            digits(&hex[2..4])?,
            digits(&hex[4..6])?,
        )),
        8 => Some(Color32::from_rgba_unmultiplied(
            digits(&hex[0..2])?,
            digits(&hex[2..4])?,
            digits(&hex[4..6])?,
            digits(&hex[6..8])?,
        )),
        _ => None,
    }
}
