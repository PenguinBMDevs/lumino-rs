//! 工程文件格式处理模块
//!
//! 处理 Lumino 工程文件格式的加载、保存、转换等功能。

pub mod project;

pub use project::{
    LoadedFileEntry, LoadedFormat, LuminoProject, TrackSlot, TrackVisibilitySer,
    archive::{ArchiveHeader, FileEntry, FileTable, build_archive, read_file_from_archive},
    data_formats::{LmctlData, LmnamesData, LmsigData, LmtempData},
    deleted_track::{
        DeletedNote, DeletedTrackData, DeletedTrackEntry, DeletedTrackMetadata, delete_permanently,
        list_deleted_tracks, load_deleted_track, save_deleted_track,
    },
    folder::FolderPaths,
    load::load_project,
    metadata::ProjectMetadata,
    save::{save_to_archive, save_to_folder},
    track::{LmtrackData, LmtrackHeader, TrackMeta},
};
