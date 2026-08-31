//! .yin 文件加载桩 — P7 约束的存储占位
//!
//! 约束（P7）：
//! - 数据模型直接用 Lumino 工程格式（`.lmpj` / MIDI），yin 格式之后适配（初期不做）
//! - 存储与 lumino 复用同一套 `EditorData` / `MidiDocument` 管线，
//!   **不新开** yin 独立文档模型，避免双轨数据分裂。
//! - 混音台不迁，多文档标签不做，i18n 按 lumino，快捷键统一
//!
//! 因此 `.yin` 分支 **暂不实现**：
//! - 未来在 `lumino-midi-loader`（`crates/audio/midi-loader`）增加 `.yin` 分支，
//!   读取 yin 二进制/压缩结构后转译为 `ParsedMidi` / `EditorData`，复用现有
//!   `import_midi_to_editor` 管线（与 `.mid` / `.lmpj` 同路径）。
//! - 初期（P7）仅做**桩式提示**：命中 `.yin` 后缀即返回友好错误，
//!   UI 侧以 `Toast` / 对话框提示“yin格式暂不支持，请导出MIDI”。
//!
//! 本文件不引入新依赖，不落盘，不持有状态，纯函数桩。

use std::path::Path;

/// yin 桩：是否命中 `.yin` 后缀（大小写不敏感）
///
/// 用于 Runner / `file_handler` 在 `ImportFiles` 阶段快速分流：
/// - `true`  → 走桩式错误（见 [`yin_load_error_message`]）
/// - `false` → 走 lumino 原有 `.mid` / `.midi` / `.lmpj` / 压缩包管线
pub fn is_yin_path(path: impl AsRef<Path>) -> bool {
    path.as_ref()
        .extension()
        .and_then(|s| s.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("yin"))
        .unwrap_or(false)
}

/// yin 桩错误文案（i18n 按 lumino：复用 `status_*` 风格，中文为主）
///
/// - 中文：`"yin格式暂不支持，请导出MIDI"`
/// - 英文：`"yin format not yet supported, please export as MIDI"`（由调用方按 `Language` 选择）
pub fn yin_load_error_message_zh() -> &'static str {
    "yin格式暂不支持，请导出MIDI"
}

pub fn yin_load_error_message_en() -> &'static str {
    "yin format not yet supported, please export as MIDI"
}

/// 按语言选择 yin 桩错误文案（复用 lumino i18n 的 `Language`）
pub fn yin_load_error_message(lang: lumino_extras::i18n::Language) -> &'static str {
    match lang {
        lumino_extras::i18n::Language::ZhCn => yin_load_error_message_zh(),
        lumino_extras::i18n::Language::EnUs => yin_load_error_message_en(),
    }
}

/// 桩式 yin 加载 — 永远返回 `Err(yin_not_supported)`
///
/// - **当前（P7）**：直接 `Err`，不读文件，不解码，不落盘，
///   调用方应在 UI 侧 `Toast` / `load_error` 对话框展示 [`yin_load_error_message`]。
/// - **未来**：在 `lumino-midi-loader` 新增 `.yin` 分支，
///
///   ```ignore
///   // crates/audio/midi-loader/src/lib.rs (future)
///   if path.extension() == Some("yin") {
///       let yin_doc = yin_parser::parse(path)?; // yin 二进制 → 中间结构
///       return yin_doc.into_parsed_midi(); // 转译为 ParsedMidi，复用 Lumino 管线
///   }
///   ```
///
///   本函数届时改为 `Ok(ParsedMidi)` 并移除桩式错误，或保留桩仅做后缀分流。
pub fn load_yin_stub(path: impl AsRef<Path>) -> Result<(), String> {
    let p = path.as_ref();
    if is_yin_path(p) {
        // 桩式：仅提示，不尝试读取文件内容，避免对未知二进制结构的误读/崩溃
        Err(yin_load_error_message_zh().to_string())
    } else {
        // 非 .yin：交由 lumino 原有管线处理，本桩不干预
        Ok(())
    }
}

/// Runner 侧便捷：尝试以 yin 桩拦截，若命中则返回 `Some(error_string)`，否则 `None`
///
/// 供 `file_handler::handle_import_files` / `MidiLoadError` 路径调用：
/// ```ignore
/// if let Some(err) = try_handle_yin_stub(&path, lang) {
///     toast.error(err); // 或 emit LoadError
///     return;
/// }
/// // 继续原有 midi 加载
/// ```
pub fn try_handle_yin_stub(
    path: impl AsRef<Path>,
    lang: lumino_extras::i18n::Language,
) -> Option<String> {
    if is_yin_path(&path) {
        Some(yin_load_error_message(lang).to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_extras::i18n::Language;
    use std::path::PathBuf;

    #[test]
    fn is_yin_detects_suffix_case_insensitive() {
        assert!(is_yin_path(PathBuf::from("song.yin")));
        assert!(is_yin_path(PathBuf::from("song.YIN")));
        assert!(is_yin_path(PathBuf::from("song.Yin")));
        assert!(!is_yin_path(PathBuf::from("song.mid")));
        assert!(!is_yin_path(PathBuf::from("song.midi")));
        assert!(!is_yin_path(PathBuf::from("song.lmpj")));
        assert!(!is_yin_path(PathBuf::from("song")));
    }

    #[test]
    fn load_stub_returns_error_for_yin() {
        let err = load_yin_stub("demo.yin").expect_err("yin 桩应返回 Err");
        assert!(err.contains("yin") || err.contains("MIDI"));
        assert!(load_yin_stub("demo.mid").is_ok());
    }

    #[test]
    fn error_message_i18n() {
        assert_eq!(
            yin_load_error_message(Language::ZhCn),
            "yin格式暂不支持，请导出MIDI"
        );
        assert!(yin_load_error_message(Language::EnUs).contains("not yet supported"));
    }

    #[test]
    fn try_handle_yin_stub_lang_aware() {
        let zh = try_handle_yin_stub("a.yin", Language::ZhCn).expect("zh yin");
        assert!(zh.contains("yin格式"));
        let en = try_handle_yin_stub("a.yin", Language::EnUs).expect("en yin");
        assert!(en.contains("not yet supported"));
        assert!(try_handle_yin_stub("a.mid", Language::ZhCn).is_none());
    }
}
