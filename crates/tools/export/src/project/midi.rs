//! `LuminoProject -> ParsedMidi` 转换，供 Runner 复用现有事件流

use std::path::PathBuf;
use std::sync::Arc;

use lumino_midi_loader::{MidiInfo, ParsedMidi};
use lumino_project::project::LuminoProject;

use crate::{ExportError, ExportResult};

/// 将 `LuminoProject` 转换为 `ParsedMidi`，供 Runner 复用现有事件流。
pub fn project_to_parsed_midi(
    project: &LuminoProject,
    original_path: impl Into<PathBuf>,
) -> ExportResult<ParsedMidi> {
    let document = project.to_midi_document().map_err(ExportError::from)?;
    let total_notes: u64 = document.notes.iter().map(|v| v.len() as u64).sum();

    let info = MidiInfo {
        path: original_path.into(),
        track_count: document.track_count,
        total_notes,
        duration_ticks: document.total_ticks,
        division: project.metadata.audio.division,
        parse_progress: Some(100.0),
    };

    Ok(ParsedMidi {
        info,
        document: Some(Arc::new(document)),
        // 历史累计创作时间随工程文件传递，供 Runner 注入会话计时器
        // （常规 MIDI 文件加载路径为 0，此处从 .lmpj metadata.stats 读取）
        accumulated_editing_secs: project.working_time_seconds(),
        // 作者/版权随工程文件传递，供 Runner 加载后恢复工程设置面板
        // （常规 MIDI 文件加载路径为空，此处从 .lmpj metadata.project 读取）
        author: project.metadata.project.author.clone(),
        copyright: project.metadata.project.copyright.clone(),
    })
}
