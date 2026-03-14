use lumino_core::ParsedMidi;
use lumino_core::midi::constants::DEFAULT_PPQN;
use std::sync::Arc;

/// 文件处理器
pub struct FileHandler {}

impl FileHandler {
    pub fn new() -> Self {
        Self {}
    }

    /// 打开文件对话框并返回选择的路径
    pub fn handle_open_file(&self) -> Option<std::path::PathBuf> {
        rfd::FileDialog::new()
            .add_filter("音乐文件", &["mid", "midi", "lmpj", "dms"])
            .add_filter("MIDI 文件", &["mid", "midi"])
            .add_filter("Lumino 项目", &["lmpj"])
            .add_filter("Domino 项目", &["dms"])
            .add_filter("所有文件", &["*"])
            .pick_file()
    }

    /// 加载 MIDI 文件
    pub async fn load_midi_file(&self, path: std::path::PathBuf) -> Result<ParsedMidi, String> {
        lumino_core::midi::loader::load_parsed_midi(path)
            .await
            .map_err(|e| e.to_string())
    }

    /// 加载 DMS 文件
    pub async fn load_dms_file(
        &self,
        path: std::path::PathBuf,
    ) -> Result<Arc<lumino_core::ParsedDms>, String> {
        lumino_core::midi::loader::load_dms(path)
            .await
            .map(Arc::new)
            .map_err(|e| e.to_string())
    }

    /// 保存为 LMPJ 文件
    pub async fn save_as_lmpj(
        &self,
        parsed: &ParsedMidi,
        path: std::path::PathBuf,
    ) -> Result<(), String> {
        lumino_export::save(parsed, path)
            .await
            .map_err(|e| e.to_string())
    }

    /// 保存为 MIDI 文件
    pub async fn save_as_midi(
        &self,
        source_path: std::path::PathBuf,
        path: std::path::PathBuf,
    ) -> Result<(), String> {
        let bytes = tokio::task::spawn_blocking(move || {
            lumino_export::export_midi_from_parsed_midi_sync(&source_path)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

        std::fs::write(&path, bytes).map_err(|e| format!("写入文件失败: {e}"))
    }

    /// 保存为 DMS 文件
    pub async fn save_as_dms(
        &self,
        source_path: std::path::PathBuf,
        path: std::path::PathBuf,
    ) -> Result<(), String> {
        let bytes = tokio::task::spawn_blocking(move || {
            lumino_export::export_dms_from_midi_sync(&source_path)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

        std::fs::write(&path, bytes).map_err(|e| format!("写入文件失败: {e}"))
    }

    /// 复制 DMS 文件
    pub async fn copy_dms_file(
        &self,
        source_path: std::path::PathBuf,
        path: std::path::PathBuf,
    ) -> Result<(), String> {
        tokio::task::spawn_blocking(move || lumino_export::copy_file_sync(&source_path, &path))
            .await
            .map_err(|e| e.to_string())?
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// 从编辑器音符构建 ParsedMidi
    pub fn build_parsed_midi_from_editor(
        &self,
        editor_notes: &[lumino_ui::TrackNotes],
        save_path: &std::path::Path,
    ) -> lumino_core::ParsedMidi {
        use lumino_core::midi::info::MidiInfo;

        let total_notes: u64 = editor_notes
            .iter()
            .map(|(_, notes)| notes.len() as u64)
            .sum();
        let max_tick = editor_notes
            .iter()
            .flat_map(|(_, notes)| notes.iter())
            .map(|(tick, _, length)| tick + length)
            .fold(0.0f32, f32::max) as u32;

        let midi_export_data = self.build_midi_export_data(editor_notes);
        let midi_bytes = match lumino_export::export_midi_to_bytes(&midi_export_data) {
            Ok(bytes) => {
                tracing::info!("生成 MIDI 字节流成功: {} 字节", bytes.len());
                if !bytes.is_empty() {
                    Some(bytes)
                } else {
                    tracing::warn!("生成的 MIDI 字节流为空");
                    None
                }
            }
            Err(e) => {
                tracing::error!("生成 MIDI 字节流失败: {}", e);
                None
            }
        };

        tracing::info!(
            "构建 ParsedMidi: 音轨数={}, 总音符数={}, midi_data={}",
            editor_notes.len(),
            total_notes,
            if midi_bytes.is_some() { "有" } else { "无" }
        );

        lumino_core::ParsedMidi {
            info: MidiInfo {
                path: save_path.to_path_buf(),
                track_count: editor_notes.len() as u16,
                total_notes,
                duration_ticks: max_tick,
                division: DEFAULT_PPQN,
                parse_progress: None,
            },
            midi_data: midi_bytes,
            memory_manager: None,
        }
    }

    /// 从编辑器音符构建 MIDI 导出数据
    pub fn build_midi_export_data(
        &self,
        editor_notes: &[lumino_ui::TrackNotes],
    ) -> lumino_export::midi::MidiExportData {
        use lumino_export::midi::{
            MidiExportData, MidiExportOptions, MidiNoteEvent, MidiTrackData,
        };

        let mut tracks = Vec::new();

        for (track_idx, notes) in editor_notes {
            let track_notes: Vec<MidiNoteEvent> = notes
                .iter()
                .map(|(tick, key, length)| MidiNoteEvent {
                    tick: *tick as u32,
                    channel: 0,
                    key: *key,
                    velocity: 100,
                    duration: *length as u32,
                })
                .collect();

            let track_data = MidiTrackData {
                notes: track_notes,
                tempos: vec![],
                program_changes: vec![],
                control_changes: vec![],
                time_signatures: vec![],
                key_signatures: vec![],
                name: Some(format!("Track {}", track_idx + 1)),
            };

            tracks.push(track_data);
        }

        MidiExportData {
            options: MidiExportOptions {
                format: 1,
                ppqn: DEFAULT_PPQN,
            },
            tracks,
        }
    }
}
