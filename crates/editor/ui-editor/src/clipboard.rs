//! 剪贴板操作：复制、剪切、粘贴音符（Lumino 程序本体间同步）
//!
//! 跨 Lumino 程序实例（多进程）的音符同步：复制时把选区写成 Lumino 私有 JSON 文本
//! （经 `arboard` 入系统剪贴板），粘贴时读回并还原。两端文档 PPQN 可能不同，因此
//! 复制载荷里携带**源 division**，粘贴时若与目标 division 不一致，就**多算一次重采样**
//! （ratio = 目标 PPQN / 源 PPQN）把 tick 偏移与音符长度等比缩放，保证粘贴出的音符
//! 与源音符「长度与数据完全一致」（同 PPQN 时零缩放、逐字节一致）。

use super::Editor;
use super::Note;
use lumino_ui_core::constants::editor::{CLIPBOARD_FORMAT, CLIPBOARD_VERSION};

#[cfg(windows)]
pub(crate) mod sys;

mod encode;
mod paste;

// 紧凑二进制编码：`build_clipboard_binary` 已跨平台开放（基准/外部复用），
// 编解码实现本身与平台无关。
use lumino_midi_model::clipboard::{ClipRecord, encode_clipboard};

#[cfg(windows)]
use lumino_midi_model::clipboard::{
    decode_clipboard_records, decode_domino_clipboard, encode_domino_clipboard,
    parse_clipboard_header,
};

/// Domino 互通剪贴板载荷的音符数上限（超过则跳过该格式，仅写 Lumino 二进制）。
///
/// 交互保护：百万级选中的 Domino 编码 ~100MB 原始体 + zlib，是复制路径的秒级成本；
/// 而 Domino 实际粘贴场景不可能承载百万级音符。Lumino 二进制（主格式）不受影响。
#[cfg(windows)]
const DOMINO_MAX_NOTES: usize = 200_000;

impl Editor {
    /// 剪切选中音符
    pub(crate) fn cut_selected_notes(&mut self) {
        if self.copy_selected_notes_to_clipboard() {
            self.delete_selected_notes();
        }
    }

    /// 复制选中音符
    pub(crate) fn copy_selected_notes(&mut self) {
        let _ = self.copy_selected_notes_to_clipboard();
    }

    /// 将选中音符复制到系统剪贴板。
    ///
    /// 优先级：Windows 下优先写入**紧凑二进制私有格式**（`LuminoMidiNotes`），
    /// 内存/速度远优于文本 JSON，且跨 Lumino 实例零拷贝；若二进制不可用，则退化为
    /// Lumino 私有 JSON 文本（跨平台正确）。两者都携带 `division`（源 PPQN），
    /// 供粘贴端做 PPQN 一致性重采样，保证「长度与数据完全一致」。
    pub(crate) fn copy_selected_notes_to_clipboard(&mut self) -> bool {
        if !self.has_selection() {
            return false;
        }
        let track = self.editor_state.data.current_track;
        let division = self
            .editor_state
            .data
            .document
            .as_ref()
            .map(|d| d.division)
            .unwrap_or(480);

        // Windows：同时写入 Lumino 私有二进制与 Domino(MidiPortalSequence) 两种格式
        #[cfg(windows)]
        {
            let domino = self.build_clipboard_domino();
            if let Some(bytes) = self.build_clipboard_binary(track, division) {
                // 优先一次会话内同时携带 Domino 格式，便于跨 DAW 粘贴
                if let Some(dom) = domino
                    && crate::clipboard::sys::set_clipboard_binary_pair(&bytes, &dom)
                {
                    tracing::info!(
                        "Editor: 已复制 {} 字节 Lumino 二进制 + {} 字节 Domino 二进制 (division={})",
                        bytes.len(),
                        dom.len(),
                        division
                    );
                    return true;
                }
                // 退化：仅 Lumino 二进制
                if crate::clipboard::sys::set_clipboard_binary(&bytes) {
                    tracing::info!(
                        "Editor: 已复制 {} 字节二进制音符 (division={})",
                        bytes.len(),
                        division
                    );
                    return true;
                }
            }
        }

        // 退化：非 Windows 或二进制不可用 → 文本 JSON
        match self.build_clipboard_json(track, division) {
            Some(s) => {
                let ok = set_clipboard_text(&s);
                if ok {
                    tracing::info!("Editor: 已复制音符 (文本 JSON, division={})", division);
                }
                ok
            }
            None => false,
        }
    }

    /// 从剪贴板粘贴音符。
    ///
    /// Windows：先探测 Lumino 二进制私有格式，命中则走紧凑二进制粘贴（含 PPQN 重采样）；
    /// 否则退化为 Lumino 私有 JSON 文本粘贴（跨平台正确）。
    pub(crate) fn paste_notes_from_clipboard(&mut self) {
        #[cfg(windows)]
        {
            if self.try_paste_from_binary() {
                return;
            }
            // Domino（TAKABO SOFT）互通：剪贴板存在 MidiPortalSequence 时，按 MIDI 标准音符解析
            if self.try_paste_from_domino() {
                return;
            }
        }
        let Some((origin_key, source_division, notes_value)) = self.read_clipboard_json() else {
            return;
        };
        if let Some((anchor, pasted)) =
            self.parse_clipboard_notes(origin_key, source_division, &notes_value)
            && !pasted.is_empty()
        {
            self.commit_pasted_notes(anchor, pasted);
        }
    }
}

/// 仅写文本到系统剪贴板（arboard，跨平台）
pub(crate) fn set_clipboard_text(text: &str) -> bool {
    let mut clipboard = match arboard::Clipboard::new() {
        Ok(cb) => cb,
        Err(e) => {
            tracing::error!("Editor: 创建剪贴板失败: {}", e);
            return false;
        }
    };
    match clipboard.set_text(text.to_string()) {
        Ok(()) => true,
        Err(e) => {
            tracing::error!("Editor: 复制到剪贴板失败: {}", e);
            false
        }
    }
}
