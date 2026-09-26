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
use std::collections::HashSet;

/// 剪贴板音符元组：`(dest_track, tick_offset, key_offset, length, velocity, channel)`
///
/// `dest_track` 为解析时已映射到**文档音轨**的索引（视觉偏移经 `document_track_at`
/// 转换得到），[`Editor::apply_clipboard_paste_entries`] 直接按此插入，不再二次换算。
/// `tick_offset` / `key_offset` 为相对载荷 origin 的偏移（未含锚点）。
pub(crate) type ClipboardNoteEntry = (usize, f32, u16, f32, u8, u8);

/// 走带二进制剪贴板子格式哨兵（写入 `ClipRecord` 头的 `track_hint` 字段）。
///
/// **语义（2026-09 跨视图互通后变更）**：从「准入门闩」降级为「子格式判别位」。
/// 粘贴端不再据此拒绝载荷（硬拒曾导致「卷帘复制 → 走带粘贴」静默失败），
/// 仅用于日志标注来源与卷帘侧选择单轨快路径 / 多轨路由。
///
/// 真正的互通基石是 `ClipRecord.track` 字段语义统一为「相对锚点的视觉轨偏移」
/// （卷侧恒 0，走带侧为 `visual - origin_visual`）——见
/// [`crate::clipboard::encode`] 与 [`crate::arrangement_ops::clipboard::encode`]。
///
/// 非 Windows 构建时二进制读写仅被单测使用（`#[cfg(windows)]` 调用点被裁剪），
/// 用 `allow(dead_code)` 避免 `-D warnings` 下的误报；Windows 下仍正常 lint。
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) const ARRANGEMENT_BINARY_MARK: u16 = 0xFFFF;

/// 多轨粘贴结果：插入数 / 是否触及当前轨 / 受影响音轨集合 / 新插入音符选区条目
pub(crate) struct PasteOutcome {
    /// 实际插入的音符数
    pub(crate) inserted: usize,
    /// 是否触及当前轨（决定是否需要额外标记当前轨脏）
    pub(crate) current_track_touched: bool,
    /// 本次实际修改的文档音轨集合（洋葱皮事件级增量用）
    pub(crate) affected_tracks: HashSet<usize>,
    /// 新插入音符的选区条目 `(视觉音轨, start_tick, end_tick, key)`
    ///
    /// 仅当调用方传 `collect_selection = true` 时填充（走带需要：粘贴后把选区
    /// 冻结为新粘贴音符，对齐卷帘 `select_notes_by_params` 的「粘贴即选中」语义）。
    /// 卷帘传 `false` 跳过——其选区是单轨索引位图，无法表达跨轨选择，省下 N×12B 分配。
    pub(crate) frozen_entries: Vec<(u16, u32, u32, u8)>,
}

#[cfg(windows)]
pub(crate) mod sys;

mod encode;
mod paste;
mod paste_interop;

// 紧凑二进制编码：`build_clipboard_binary` 已跨平台开放（基准/外部复用），
// 编解码实现本身与平台无关。
use lumino_midi_model::clipboard::{ClipRecord, encode_clipboard};

// 编解码导入跨平台：`parse_clipboard_header` / `decode_clipboard_records` 被纯函数
// `paste_binary_payload` 使用（无平台依赖），非 Windows 下也必须可用。
// 仅 `domino` 三件套（`decode_domino_clipboard` / `encode_domino_clipboard`）仍限 Windows
// ——它们只服务于 Windows 原生剪贴板互操作。
use lumino_midi_model::clipboard::{decode_clipboard_records, parse_clipboard_header};

#[cfg(windows)]
use lumino_midi_model::clipboard::{decode_domino_clipboard, encode_domino_clipboard};

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
    ///
    /// **跨视图互通**：走带子格式载荷（`type="arrangement"` / 走带二进制哨兵）在两条
    /// 路径上都会被识别并按 per-note 轨偏移**多轨路由**；卷帘子格式走原单轨快路径。
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
        // 统一 JSON 入口：内部按 `type` 判别子格式（走带多轨 / 卷帘单轨）
        if let Ok(text) = crate::clipboard::read_clipboard_text() {
            self.paste_json_payload(&text);
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

/// 仅从系统剪贴板读文本（arboard，跨平台）
pub(crate) fn read_clipboard_text() -> Result<String, arboard::Error> {
    arboard::Clipboard::new()?.get_text()
}
