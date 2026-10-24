//! MIDI 文件输入层：只读映射优先，失败回退整份读取。
//!
//! 编辑器主链路此前用 `tokio::fs::read` 把整份文件读成 `Vec<u8>` 再交给
//! [`MidiDocument::from_notes_bytes`]，黑乐谱（1.31 GB / 1.64 亿音符）在加载期会
//! 多出一份与文件等大的**私有堆副本**。解析器本来就接收 `&[u8]`，因此把
//! 「文件 → 字节」换成 `memmap2::Mmap` 即可，解析逻辑零改动。
//!
//! ## 生命周期（关键约束）
//!
//! 映射**只活在本次解析窗口内**：`Mmap` 在 [`load_document_from_path`] 返回前 drop，
//! 绝不放进 [`crate::ParsedMidi`] 或 [`MidiDocument`]。文档保留自己的数据副本，
//! 文件句柄不会滞留（否则会阻止该文件被删除/覆盖）。
//!
//! ## 已知边界
//!
//! 映射期间其它进程截断/改写该文件属未定义行为（`memmap2` 文档已警告）。本模块
//! 只做短窗口 + 失败回退，不做跨平台文件锁；编辑器加载窗口内不应存在同路径写入。

use std::fs::File;
use std::path::Path;

use lumino_core::{tr, trf};

use crate::{LoaderError, LoaderResult, MidiDocument};

/// 文件输入方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InputMode {
    /// 生产路径：先尝试只读映射，失败回退整份读取。
    MmapFirst,
    /// 整份读取（改造前的行为）。测试用它验证回退路径与逐项等价；
    /// 运行时也可用 `LUMINO_MIDI_INPUT=read` 强制启用以做前后对照/排障。
    ReadOnly,
}

/// 解析运行时输入方式。
///
/// `LUMINO_MIDI_INPUT=read` 可临时禁用映射（性能对照、特殊文件系统排障）。
/// 与 `LUMINO_MEMORY_SOFT_LIMIT_MB` 同类：只在诊断/CI 场景使用，默认不设置。
fn input_mode_from_env() -> InputMode {
    match std::env::var("LUMINO_MIDI_INPUT").ok().as_deref() {
        Some("read") => InputMode::ReadOnly,
        _ => InputMode::MmapFirst,
    }
}

/// 从磁盘路径解析 MIDI 为紧凑文档，返回 `(文档, division, 音符总数)`。
///
/// 输入方式由 [`input_mode_from_env`] 决定（默认映射）。
///
/// # 错误
/// * 路径不存在 / 无权限 → [`LoaderError::Io`]
/// * 非普通文件（目录、设备等）或 0 字节文件 → [`LoaderError::FileFormat`]
/// * 解析失败 → [`LoaderError::MidiParse`]（保留改造前的错误文案）
pub(super) fn load_document_from_path(
    path: &Path,
    progress: Option<&dyn Fn(f64)>,
) -> LoaderResult<(MidiDocument, u16, u64)> {
    load_with_mode(path, progress, input_mode_from_env())
}

/// 指定输入方式的实现（测试直接调用以覆盖两条路径）。
fn load_with_mode(
    path: &Path,
    progress: Option<&dyn Fn(f64)>,
    mode: InputMode,
) -> LoaderResult<(MidiDocument, u16, u64)> {
    load_with_mapper(path, progress, mode, map_read_only)
}

/// 可注入映射器的实现：`mapper` 只在 `MmapFirst` 下被调用。
///
/// 之所以把映射器做成参数，是为了让「映射失败 → 整份读取」这条回退分支可被单测
/// 直接覆盖（真实环境里很难稳定造出 `Mmap::map` 失败）。
fn load_with_mapper(
    path: &Path,
    progress: Option<&dyn Fn(f64)>,
    mode: InputMode,
    mapper: impl Fn(&Path) -> std::io::Result<memmap2::Mmap>,
) -> LoaderResult<(MidiDocument, u16, u64)> {
    let meta = std::fs::metadata(path).map_err(LoaderError::Io)?;
    // 目录/设备等非普通文件必须先拦掉：Windows 上目录的 metadata().len() 为 0，
    // 若直接走空文件分支会给出误导性的「文件为空」。
    if !meta.is_file() {
        return Err(LoaderError::FileFormat(
            tr!("不是普通文件，无法按 MIDI 加载").to_string(),
        ));
    }
    // 0 字节文件必须先于映射拦掉：映射 0 长度在 Windows（ERROR_FILE_INVALID）与
    // Linux（EINVAL）都会失败，走「失败 → 回退读取」只会绕一圈再报错。
    if meta.len() == 0 {
        return Err(LoaderError::FileFormat(tr!("MIDI 文件为空").to_string()));
    }

    if mode == InputMode::MmapFirst {
        match mapper(path) {
            Ok(mmap) => {
                tracing::info!("MIDI 输入: mmap path={} len={}", path.display(), meta.len());
                let parsed = parse_bytes(&mmap, progress);
                // 显式释放映射：文档已完全拥有自己的数据，句柄不应滞留。
                drop(mmap);
                return parsed;
            }
            Err(e) => tracing::warn!(
                "MIDI mmap 失败，回退整份读取: {e}（path={}）",
                path.display()
            ),
        }
    }

    tracing::info!(
        "MIDI 输入: fs::read path={} len={}",
        path.display(),
        meta.len()
    );
    let bytes = std::fs::read(path).map_err(LoaderError::Io)?;
    parse_bytes(&bytes, progress)
}

/// 解析字节并保持改造前的错误文案（解析类失败统一归入 `MidiParse`）。
fn parse_bytes(
    bytes: &[u8],
    progress: Option<&dyn Fn(f64)>,
) -> LoaderResult<(MidiDocument, u16, u64)> {
    MidiDocument::from_notes_bytes(bytes, progress)
        .map_err(|e| LoaderError::MidiParse(trf!("解析 MIDI 数据失败: {e}", e = e)))
}

/// 建立只读映射。
fn map_read_only(path: &Path) -> std::io::Result<memmap2::Mmap> {
    let file = File::open(path)?;
    // SAFETY: 只读映射，且映射生命周期严格限制在本函数返回后的解析窗口内
    // （`load_document_from_path` 解析完立即 drop）。本进程不写该文件。
    // 残余风险：解析窗口内其它进程截断/改写该文件属未定义行为（memmap2 文档已
    // 明确警告）；本卡不做跨平台文件锁，属已披露的接受项。
    unsafe { memmap2::Mmap::map(&file) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_vlq(out: &mut Vec<u8>, mut v: u32) {
        let mut bytes = vec![(v & 0x7f) as u8];
        v >>= 7;
        while v > 0 {
            bytes.push(((v & 0x7f) as u8) | 0x80);
            v >>= 7;
        }
        bytes.reverse();
        out.extend_from_slice(&bytes);
    }

    fn push_meta(out: &mut Vec<u8>, delta: u32, ty: u8, data: &[u8]) {
        push_vlq(out, delta);
        out.push(0xFF);
        out.push(ty);
        push_vlq(out, data.len() as u32);
        out.extend_from_slice(data);
    }

    /// 双轨 SMF：覆盖音轨名/端口/速度/拍号/调号/文本/歌词/标记/SysEx/CC/音符。
    fn fixture() -> Vec<u8> {
        let mut midi = Vec::new();
        midi.extend_from_slice(b"MThd");
        midi.extend_from_slice(&6u32.to_be_bytes());
        midi.extend_from_slice(&1u16.to_be_bytes());
        midi.extend_from_slice(&2u16.to_be_bytes());
        midi.extend_from_slice(&480u16.to_be_bytes());

        let mut conductor = Vec::new();
        push_meta(&mut conductor, 0, 0x03, b"Conductor");
        push_meta(&mut conductor, 0, 0x21, &[2]);
        push_meta(&mut conductor, 0, 0x51, &[0x07, 0xA1, 0x20]);
        push_meta(&mut conductor, 0, 0x58, &[3, 2, 24, 8]);
        push_meta(&mut conductor, 0, 0x59, &[0xFF, 0]); // -1 个降号，大调
        push_meta(&mut conductor, 0, 0x01, b"hello text");
        push_meta(&mut conductor, 0, 0x05, b"la");
        push_meta(&mut conductor, 0, 0x06, b"mark");
        push_vlq(&mut conductor, 0);
        conductor.push(0xF0);
        push_vlq(&mut conductor, 4);
        conductor.extend_from_slice(&[0x7E, 0x7F, 0x09, 0xF7]);
        push_vlq(&mut conductor, 0);
        conductor.push(0xB0);
        conductor.push(7);
        conductor.push(100);
        push_vlq(&mut conductor, 0);
        conductor.push(0x90);
        conductor.push(60);
        conductor.push(64);
        push_vlq(&mut conductor, 96);
        conductor.push(0x80);
        conductor.push(60);
        conductor.push(32);

        midi.extend_from_slice(b"MTrk");
        midi.extend_from_slice(&(conductor.len() as u32).to_be_bytes());
        midi.extend_from_slice(&conductor);

        let mut piano = Vec::new();
        push_meta(&mut piano, 0, 0x03, b"Piano");
        push_meta(&mut piano, 0, 0x21, &[0]);
        for (i, key) in [64u8, 67, 72].into_iter().enumerate() {
            push_vlq(&mut piano, 0);
            piano.push(0x91);
            piano.push(key);
            piano.push(80 + i as u8);
            push_vlq(&mut piano, 240);
            piano.push(0x81);
            piano.push(key);
            piano.push(0);
        }
        push_vlq(&mut piano, 0);
        piano.push(0xB1);
        piano.push(64);
        piano.push(127);

        midi.extend_from_slice(b"MTrk");
        midi.extend_from_slice(&(piano.len() as u32).to_be_bytes());
        midi.extend_from_slice(&piano);
        midi
    }

    fn temp_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("lumino_mmap_{}_{}.mid", std::process::id(), tag))
    }

    fn write_temp(tag: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = temp_path(tag);
        std::fs::write(&path, bytes).expect("测试：写入临时 MIDI 失败");
        path
    }

    fn note_snapshot(doc: &MidiDocument) -> Vec<Vec<lumino_midi_model::NoteEvent>> {
        (0..doc.track_count as usize)
            .map(|t| doc.notes[t].iter().copied().collect())
            .collect()
    }

    #[test]
    fn test_mmap_and_read_produce_identical_document() {
        let path = write_temp("equiv", &fixture());

        let (mmap_doc, mmap_div, mmap_notes) =
            load_with_mode(&path, None, InputMode::MmapFirst).expect("测试：mmap 路径加载失败");
        let (read_doc, read_div, read_notes) =
            load_with_mode(&path, None, InputMode::ReadOnly).expect("测试：整读路径加载失败");

        let _ = std::fs::remove_file(&path);

        assert_eq!(mmap_div, read_div, "division 不一致");
        assert_eq!(mmap_notes, read_notes, "音符总数不一致");
        assert!(mmap_notes > 0, "测试素材应包含音符");
        assert_eq!(mmap_doc.track_count, read_doc.track_count, "音轨数不一致");
        assert_eq!(mmap_doc.total_ticks, read_doc.total_ticks, "总 tick 不一致");
        assert_eq!(
            mmap_doc.tempo_changes, read_doc.tempo_changes,
            "tempo 不一致"
        );
        assert_eq!(
            mmap_doc.time_signatures, read_doc.time_signatures,
            "拍号不一致"
        );
        assert_eq!(
            mmap_doc.key_signatures, read_doc.key_signatures,
            "调号不一致"
        );
        assert_eq!(mmap_doc.lyrics, read_doc.lyrics, "歌词不一致");
        assert_eq!(mmap_doc.markers, read_doc.markers, "标记不一致");
        assert_eq!(mmap_doc.text_events, read_doc.text_events, "文本事件不一致");
        assert_eq!(mmap_doc.sys_ex, read_doc.sys_ex, "SysEx 不一致");
        assert_eq!(mmap_doc.track_names, read_doc.track_names, "音轨名不一致");
        assert_eq!(mmap_doc.track_ports, read_doc.track_ports, "端口不一致");
        let mmap_cc: Vec<_> = mmap_doc.control_events.iter().copied().collect();
        let read_cc: Vec<_> = read_doc.control_events.iter().copied().collect();
        assert_eq!(mmap_cc, read_cc, "控制事件不一致");
        assert_eq!(
            note_snapshot(&mmap_doc),
            note_snapshot(&read_doc),
            "音符不一致"
        );
    }

    #[test]
    fn test_mapper_failure_falls_back_to_full_read() {
        let path = write_temp("fallback", &fixture());

        // 注入必然失败的映射器：回退分支应成功，且文档与整读路径一致。
        let (doc, division, notes) = load_with_mapper(&path, None, InputMode::MmapFirst, |_| {
            Err(std::io::Error::other("模拟映射失败"))
        })
        .expect("测试：映射失败后应回退整份读取成功");
        let (read_doc, read_div, read_notes) =
            load_with_mode(&path, None, InputMode::ReadOnly).expect("测试：整读路径加载失败");

        let _ = std::fs::remove_file(&path);

        assert_eq!(division, read_div, "division 不一致");
        assert_eq!(notes, read_notes, "音符总数不一致");
        assert_eq!(doc.total_ticks, read_doc.total_ticks, "总 tick 不一致");
        assert_eq!(doc.tempo_changes, read_doc.tempo_changes, "tempo 不一致");
        assert_eq!(doc.track_names, read_doc.track_names, "音轨名不一致");
        assert_eq!(
            note_snapshot(&doc),
            note_snapshot(&read_doc),
            "回退路径音符不一致"
        );
    }

    #[test]
    fn test_empty_file_is_format_error() {
        let path = write_temp("empty", b"");
        let err =
            load_with_mode(&path, None, InputMode::MmapFirst).expect_err("测试：空文件应返回错误");
        let _ = std::fs::remove_file(&path);
        assert!(
            matches!(err, LoaderError::FileFormat(_)),
            "空文件应返回 FileFormat，实际: {err:?}"
        );
    }

    #[test]
    fn test_missing_file_is_io_error() {
        let path = temp_path("missing");
        let err = load_with_mode(&path, None, InputMode::MmapFirst)
            .expect_err("测试：不存在的文件应返回错误");
        assert!(
            matches!(err, LoaderError::Io(_)),
            "不存在的文件应返回 Io，实际: {err:?}"
        );
    }

    #[test]
    fn test_directory_is_format_error() {
        let dir = std::env::temp_dir();
        let err =
            load_with_mode(&dir, None, InputMode::MmapFirst).expect_err("测试：目录应返回错误");
        assert!(
            matches!(err, LoaderError::FileFormat(_)),
            "目录应返回 FileFormat，实际: {err:?}"
        );
    }

    #[test]
    fn test_truncated_midi_reports_parse_error_in_both_modes() {
        // 只有 4 字节的伪文件：映射/整读都应走到同一条解析失败分支。
        let path = write_temp("truncated", b"MThd");
        let mmap_err = load_with_mode(&path, None, InputMode::MmapFirst)
            .expect_err("测试：截断文件应返回错误");
        let read_err =
            load_with_mode(&path, None, InputMode::ReadOnly).expect_err("测试：截断文件应返回错误");
        let _ = std::fs::remove_file(&path);
        assert!(
            matches!(mmap_err, LoaderError::MidiParse(_)),
            "mmap 路径截断文件应返回 MidiParse，实际: {mmap_err:?}"
        );
        assert!(
            matches!(read_err, LoaderError::MidiParse(_)),
            "整读路径截断文件应返回 MidiParse，实际: {read_err:?}"
        );
    }
}
