//! 归档形态工程加载

use std::path::PathBuf;

use crate::project::{
    archive,
    data_formats::{LmctlData, LmnamesData, LmsigData, LmsyxData, LmtempData, LmtxtData},
    metadata::ProjectMetadata,
};
use crate::{LmtrackData, LuminoProject, TrackSlot};
use lumino_core::error::{CoreError, Result};

/// 从归档文件加载
pub(super) fn load_from_archive(bytes: &[u8]) -> Result<LuminoProject> {
    // 读取 metadata.toml
    let meta_bytes = archive::read_file_from_archive(bytes, "metadata.toml")
        .map_err(|e| CoreError::FileFormat(format!("归档读取失败: {e}")))?
        .ok_or_else(|| CoreError::FileFormat("归档中缺少 metadata.toml".into()))?;
    let metadata = ProjectMetadata::from_toml_str(
        std::str::from_utf8(&meta_bytes)
            .map_err(|e| CoreError::FileFormat(format!("metadata 编码错误: {e}")))?,
    )
    .map_err(|e| CoreError::FileFormat(format!("metadata 解析失败: {e}")))?;

    let mut project = LuminoProject::new(&metadata.project.name);
    project.metadata = metadata;

    // 读取音轨（根据 metadata 中的 track_count）
    for track_id in 0..project.metadata.audio.track_count {
        let path = format!("data/project/tracks/{:03}.lmtrack", track_id);
        if let Some(track_bytes) = archive::read_file_from_archive(bytes, &path)
            .map_err(|e| CoreError::FileFormat(format!("读取音轨 {track_id} 失败: {e}")))?
        {
            let track = LmtrackData::decode(&track_bytes)
                .map_err(|e| CoreError::FileFormat(format!("解码音轨 {track_id} 失败: {e}")))?;
            let idx = track_id as usize;
            if idx >= project.tracks.len() {
                project.tracks.resize_with(idx + 1, || TrackSlot::Unloaded {
                    track_id: 0,
                    path: PathBuf::new(),
                });
            }
            project.tracks[idx] = TrackSlot::Loaded(track);
        }
    }

    // 读取 tempo（专用格式 LMTM）
    if let Some(tempo_bytes) =
        archive::read_file_from_archive(bytes, "data/project/tempo.lmtemp")
            .map_err(|e| CoreError::FileFormat(format!("读取 tempo 失败: {e}")))?
    {
        let data = LmtempData::decode(&tempo_bytes)
            .map_err(|e| CoreError::FileFormat(format!("tempo 解码失败: {e}")))?;
        project.tempo_changes = data.tempo_changes;
    }

    // 读取 signature（专用格式 LMSG）
    if let Some(sig_bytes) = archive::read_file_from_archive(bytes, "data/project/signature.lmsig")
        .map_err(|e| CoreError::FileFormat(format!("读取 signature 失败: {e}")))?
    {
        let data = LmsigData::decode(&sig_bytes)
            .map_err(|e| CoreError::FileFormat(format!("signature 解码失败: {e}")))?;
        project.time_signatures = data.time_signatures;
        project.key_signatures = data.key_signatures;
    }

    // 读取 controls（专用格式 LMCT）
    if let Some(ctl_bytes) =
        archive::read_file_from_archive(bytes, "data/project/controls.lmctl")
            .map_err(|e| CoreError::FileFormat(format!("读取 controls 失败: {e}")))?
    {
        let data = LmctlData::decode(&ctl_bytes)
            .map_err(|e| CoreError::FileFormat(format!("controls 解码失败: {e}")))?;
        project.control_changes = data.control_changes;
        project.program_changes = data.program_changes;
        project.pitch_bends = data.pitch_bends;
    }

    // 读取 text events（专用格式 LMTX）
    if let Some(txt_bytes) =
        archive::read_file_from_archive(bytes, "data/project/text_events.lmtxt")
            .map_err(|e| CoreError::FileFormat(format!("读取 text events 失败: {e}")))?
    {
        let data = LmtxtData::decode(&txt_bytes)
            .map_err(|e| CoreError::FileFormat(format!("text events 解码失败: {e}")))?;
        project.lyrics = data.lyrics;
        project.markers = data.markers;
    }

    // 读取 SysEx（专用格式 LMSY）
    if let Some(syx_bytes) = archive::read_file_from_archive(bytes, "data/project/sysex.lmsyx")
        .map_err(|e| CoreError::FileFormat(format!("读取 SysEx 失败: {e}")))?
    {
        let data = LmsyxData::decode(&syx_bytes)
            .map_err(|e| CoreError::FileFormat(format!("SysEx 解码失败: {e}")))?;
        project.sys_ex = data.sys_ex;
    }

    // 读取 track_names（专用格式 LMNM）
    if let Some(names_bytes) =
        archive::read_file_from_archive(bytes, "data/project/track_names.lmnames")
            .map_err(|e| CoreError::FileFormat(format!("读取 names 失败: {e}")))?
    {
        let _data = LmnamesData::decode(&names_bytes)
            .map_err(|e| CoreError::FileFormat(format!("names 解码失败: {e}")))?;
    }

    Ok(project)
}
