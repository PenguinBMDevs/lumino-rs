//! 工程事件/元数据部件的共用解码（9 段）
//!
//! 文件夹形态与归档形态的差异只在「按相对路径取字节」；解码、缺失即空、
//! 字段赋值逐段相同。此模块把这段唯一化，避免两侧漏改。

use crate::LuminoProject;
use crate::project::data_formats::{
    LmcatData, LmctlData, LmnamesData, LmshapesData, LmsigData, LmsyxData, LmtempData,
    LmtextmetaData, LmtxtData,
};
use crate::project::folder::FolderPaths;
use lumino_core::error::{CoreError, Result};
use lumino_core::trf;

/// 解码 9 个事件/元数据部件并写入 `project`。
///
/// `read` 按相对路径返回部件字节；`None` 表示部件缺失（老工程），保持对应
/// 字段为空、不报错。
pub(super) fn apply_event_parts(
    project: &mut LuminoProject,
    mut read: impl FnMut(&str) -> Result<Option<Vec<u8>>>,
) -> Result<()> {
    // tempo（专用格式 LMTM）
    if let Some(bytes) = read(FolderPaths::TEMPO_FILE)? {
        let data = LmtempData::decode(&bytes)
            .map_err(|e| CoreError::FileFormat(trf!("tempo 解码失败: {e}", e = e)))?;
        project.tempo_changes = data.tempo_changes;
    }

    // signature（专用格式 LMSG）
    if let Some(bytes) = read(FolderPaths::SIGNATURE_FILE)? {
        let data = LmsigData::decode(&bytes)
            .map_err(|e| CoreError::FileFormat(trf!("signature 解码失败: {e}", e = e)))?;
        project.time_signatures = data.time_signatures;
        project.key_signatures = data.key_signatures;
    }

    // controls（专用格式 LMCT）
    if let Some(bytes) = read(FolderPaths::CONTROLS_FILE)? {
        let data = LmctlData::decode(&bytes)
            .map_err(|e| CoreError::FileFormat(trf!("controls 解码失败: {e}", e = e)))?;
        project.control_changes = data.control_changes;
        project.program_changes = data.program_changes;
        project.pitch_bends = data.pitch_bends;
    }

    // 触后（专用格式 LMAT；老工程无此文件即空，不报错）
    if let Some(bytes) = read(FolderPaths::AFTERTOUCH_FILE)? {
        let data = LmcatData::decode(&bytes)
            .map_err(|e| CoreError::FileFormat(trf!("aftertouch 解码失败: {e}", e = e)))?;
        project.channel_aftertouch = data.channel_aftertouch;
        project.poly_aftertouch = data.poly_aftertouch;
    }

    // text events（专用格式 LMTX）
    if let Some(bytes) = read(FolderPaths::TEXT_EVENTS_FILE)? {
        let data = LmtxtData::decode(&bytes)
            .map_err(|e| CoreError::FileFormat(trf!("text events 解码失败: {e}", e = e)))?;
        project.lyrics = data.lyrics;
        project.markers = data.markers;
    }

    // text metas（专用格式 LMMT；老工程无此文件即空，不报错）
    if let Some(bytes) = read(FolderPaths::TEXT_METAS_FILE)? {
        let data = LmtextmetaData::decode(&bytes)
            .map_err(|e| CoreError::FileFormat(trf!("text metas 解码失败: {e}", e = e)))?;
        project.text_events = data.text_events;
    }

    // SysEx（专用格式 LMSY）
    if let Some(bytes) = read(FolderPaths::SYSEX_FILE)? {
        let data = LmsyxData::decode(&bytes)
            .map_err(|e| CoreError::FileFormat(trf!("SysEx 解码失败: {e}", e = e)))?;
        project.sys_ex = data.sys_ex;
    }

    // track_names（专用格式 LMNM）：只校验，名称实际来自各 .lmtrack
    if let Some(bytes) = read(FolderPaths::TRACK_NAMES_FILE)? {
        let _data = LmnamesData::decode(&bytes)
            .map_err(|e| CoreError::FileFormat(trf!("names 解码失败: {e}", e = e)))?;
    }

    // 形状（专用格式 LMSH；老工程无此文件即空，不报错）
    if let Some(bytes) = read(FolderPaths::SHAPES_FILE)? {
        let data = LmshapesData::decode(&bytes)
            .map_err(|e| CoreError::FileFormat(trf!("形状解码失败: {e}", e = e)))?;
        project.shapes = data.shapes;
    }

    Ok(())
}
