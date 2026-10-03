//! 去 ID 后音符体积说明示例
//!
//! 回答问题：去掉音符全局 ID 后，内存与工程文件省了多少？
//!
//! - 内存：`NoteEvent` 去 ID 后为 12 字节（2×u32 + 4×u8），由本示例与
//!   `note_event.rs` 单测双重锁死；若保留 `u64` ID 则为 20 字节/音符。
//! - 工程文件：现状基线（`save_to_archive`）本来就不存 ID；本示例保存基线
//!   工程并报告大小，同时按「每音符 8 字节 ID（bincode 原样）」估算含 ID
//!   版本的理论裸增量，说明 zstd 压缩前后的量级关系。
//!
//! # 用法
//! ```bash
//! cargo run -p lumino-project --example note_id_storage_size_compare
//! cargo run -p lumino-project --example note_id_storage_size_compare -- --midi test-file/test_unzip_midi/Erosoul.mid
//! ```
//!
//! 默认 MIDI 为仓库测试资源 `test-file/test_unzip_midi/Erosoul.mid`（约 16 MB）。

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lumino_midi_model::{MidiDocument, NoteEvent};
use lumino_project::LuminoProject;

/// 解析 `--midi <path>`，缺省用仓库测试 MIDI。
fn resolve_midi_arg() -> PathBuf {
    let args: Vec<String> = std::env::args().collect();
    let mut idx = 0;
    while idx < args.len() {
        if args[idx] == "--midi"
            && let Some(p) = args.get(idx + 1)
        {
            return PathBuf::from(p);
        }
        idx += 1;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../test-file/test_unzip_midi/Erosoul.mid")
}

fn file_size(path: &Path) -> Result<u64, String> {
    std::fs::metadata(path)
        .map(|m| m.len())
        .map_err(|e| format!("读取文件大小失败 {}: {e}", path.display()))
}

fn run() -> Result<(), String> {
    // 去 ID 后 12 字节/音符（2×u32 + 4×u8），与单测同口径锁死。
    assert_eq!(
        core::mem::size_of::<NoteEvent>(),
        12,
        "NoteEvent 去 ID 后必须为 12 字节"
    );

    let midi_path = resolve_midi_arg();
    if !midi_path.exists() {
        return Err(format!("测试 MIDI 不存在: {}", midi_path.display()));
    }
    let midi_len = file_size(&midi_path)?;
    println!("[1/3] 加载 MIDI（项目接口 MidiDocument::from_notes_file）");
    println!("      路径: {}", midi_path.display());
    println!(
        "      源大小: {midi_len} bytes ({:.2} MB)",
        midi_len as f64 / 1_048_576.0
    );
    let doc = MidiDocument::from_notes_file(&midi_path, None)
        .map_err(|e| format!("MIDI 加载失败: {e}"))?;
    let total_notes: usize = (0..doc.track_count)
        .map(|t| doc.track_notes(t as usize).len())
        .sum();
    println!(
        "      音轨: {} 轨, 音符: {} 个, 单音符: {} bytes（去 ID 后）",
        doc.track_count,
        total_notes,
        core::mem::size_of::<NoteEvent>()
    );
    if total_notes == 0 {
        return Err("MIDI 中无音符，无法对比".into());
    }

    println!("[2/3] 构建工程（项目接口 LuminoProject::from_midi_document）");
    let project = LuminoProject::from_midi_document(&doc);

    let out_dir = std::env::temp_dir().join("lumino_note_id_compare");
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("创建输出目录失败: {e}"))?;
    let baseline_path = out_dir.join("baseline.lmpj");

    println!("[3/3] 保存基线工程（项目接口 save_to_archive，不存 ID）");
    lumino_project::save_to_archive(&project, &baseline_path)
        .map_err(|e| format!("基线保存失败: {e}"))?;
    let baseline_len = file_size(&baseline_path)?;

    // 理论对照：若每音符多存 8 字节 ID（bincode 原样），裸增量即下值；
    // ID 单调递增高度可压缩，zstd 后实际开销远小于此。
    let raw_ids = total_notes as u64 * 8;
    let mem_12 = total_notes as u64 * 12;
    let mem_20 = total_notes as u64 * 20;
    println!("────────── 对比结果 ──────────");
    println!(
        "基线 .lmpj : {baseline_len} bytes ({:.2} MB)",
        baseline_len as f64 / 1_048_576.0
    );
    println!("内存占用   : {mem_12} bytes（{total_notes} 音符 × 12 bytes，去 ID 后）");
    println!("含 ID 内存 : {mem_20} bytes（同量 × 20 bytes，+8 ID），省约 {raw_ids} bytes");
    println!("裸 ID 总量 : {raw_ids} bytes（若存 ID，压缩前至少多这些）");
    println!("输出目录   : {}", out_dir.display());
    println!("结论：去 ID 后单音符 12 字节；工程文件本来就不存 ID，基线即最终形态。");
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("对比失败: {e}");
            ExitCode::FAILURE
        }
    }
}
