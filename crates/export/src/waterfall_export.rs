//! 贴图瀑布流导出
//!
//! 将 `LuminoProject` 的音轨数据转换为 `WaterfallNote` 并生成 `.lmocache`
//! 贴图缓存到工程文件夹的 `data/image` 目录。

use std::collections::HashMap;
use std::path::Path;

use lumino_extras::palette::onion_track_color;
use lumino_midi_model::compact::{CompactEvent, EventKind};
use lumino_midiplayer::texture_waterfall::WaterfallNote;
use lumino_midiplayer::texture_waterfall::{TextureWaterfallConfig, generate_waterfall_tiles};
use lumino_project::project::metadata::ImageMetadata;
use lumino_project::project::{LuminoProject, TrackSlot};

use crate::ExportResult;

/// 导出项目贴图瀑布流瀑布流贴图到 `data/image`
///
/// 使用项目固定哈希作为缓存分桶，保证同一工程加载时缓存命中。
/// 当 `LuminoProject` 中无已加载音轨时，不生成任何文件，仅返回默认元数据。
pub fn export_waterfall_tiles(
    project: &LuminoProject,
    image_dir: impl AsRef<Path>,
    cache_hash: &str,
    key_count: u16,
) -> ExportResult<ImageMetadata> {
    let image_dir = image_dir.as_ref();
    std::fs::create_dir_all(image_dir)?;

    let mut notes = collect_onion_skin_notes(project);
    let config = TextureWaterfallConfig::default();

    if notes.is_empty() {
        return Ok(ImageMetadata {
            cache_hash: cache_hash.to_string(),
            tile_width_px: config.tile_width_px,
            key_count,
            measures_per_group: config.measures_per_group,
        });
    }

    let ppq = project.metadata.audio.division;
    let total_ticks = project.metadata.audio.total_ticks;

    let mut export_config = config;
    export_config.cache_dir = image_dir.to_path_buf();

    // generate_waterfall_tiles 内部已启用后台缓存写入线程，会在生成的同时
    // 将每个 TrackTile 写入 `cache_dir/{hash}_t{idx}_g{group}.lmocache`。
    let tiles = generate_waterfall_tiles(
        &mut notes,
        &export_config,
        ppq,
        key_count,
        total_ticks,
        cache_hash,
        None,
    );

    // 丢弃内存中的 GroupTile，导出只依赖磁盘缓存文件。
    drop(tiles);

    Ok(ImageMetadata {
        cache_hash: cache_hash.to_string(),
        tile_width_px: export_config.tile_width_px,
        key_count,
        measures_per_group: export_config.measures_per_group,
    })
}

/// 收集所有已加载或已修改音轨的贴图瀑布流音符
fn collect_onion_skin_notes(project: &LuminoProject) -> Vec<Vec<WaterfallNote>> {
    project
        .tracks
        .iter()
        .enumerate()
        .filter_map(|(idx, slot)| {
            let data = match slot {
                TrackSlot::Loaded(d) | TrackSlot::Modified(d) => d,
                TrackSlot::Unloaded { .. } => return None,
            };
            Some(track_notes_from_lmtrack(data, idx as u16))
        })
        .collect()
}

/// 从单轨 `LmtrackData` 提取 `WaterfallNote` 列表
///
/// `CompactEvent` 按 delta_tick 排序，通过 NoteOn/NoteOff 配对得到音符起止。
fn track_notes_from_lmtrack(
    data: &lumino_project::project::LmtrackData,
    track_idx: u16,
) -> Vec<WaterfallNote> {
    let events: Vec<CompactEvent> = match data.compact_events() {
        Ok(iter) => iter.collect(),
        Err(e) => {
            tracing::warn!("提取音轨 {track_idx} 事件失败: {e}");
            return Vec::new();
        }
    };

    let mut active: HashMap<(u16, u8), u32> = HashMap::new();
    let mut notes = Vec::new();
    let mut current_tick = 0_u32;
    let color = onion_track_color(track_idx as usize);

    for ev in events {
        current_tick = current_tick.saturating_add(ev.delta_tick());
        let key = ev.param1();
        let channel = ev.channel();
        let kind = ev.kind();

        if kind == EventKind::NoteOn && ev.param2() > 0 {
            active.insert((key, channel), current_tick);
        } else if (kind == EventKind::NoteOff || (kind == EventKind::NoteOn && ev.param2() == 0))
            && let Some(start) = active.remove(&(key, channel))
        {
            notes.push(WaterfallNote::from_ms(
                start as f32,
                current_tick as f32,
                key as u8,
                color,
            ));
        }
    }

    // 未关闭的音符延伸到该音轨最大 tick
    let max_tick = data.meta.max_tick;
    if max_tick > 0 {
        for ((key, _), start) in active {
            notes.push(WaterfallNote::from_ms(
                start as f32,
                max_tick as f32,
                key as u8,
                color,
            ));
        }
    }

    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_midi_model::compact::CompactEvent;
    use lumino_project::project::LmtrackData;
    use lumino_project::project::TrackVisibilitySer;
    use lumino_project::project::track::TrackMeta;
    use tempfile::tempdir;

    fn make_track_with_note(meta_max_tick: u32) -> LmtrackData {
        let events = vec![
            CompactEvent::new(0, 0, EventKind::NoteOn, 0, 60, 100),
            CompactEvent::new(480, 0, EventKind::NoteOff, 0, 60, 0),
        ];
        LmtrackData::from_compact_events(
            TrackMeta {
                track_id: 0,
                name: "Piano".into(),
                channel: 0,
                port: 0,
                visibility: TrackVisibilitySer::Visible,
                solo: false,
                is_drum: false,
                max_tick: meta_max_tick,
            },
            &events,
        )
    }

    #[test]
    fn test_track_notes_from_lmtrack_pairs_note_on_off() {
        let track = make_track_with_note(480);
        let notes = track_notes_from_lmtrack(&track, 0);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].key, 60);
        assert_eq!(notes[0].start_ms, 0.0);
        assert_eq!(notes[0].end_ms, 480.0);
    }

    #[test]
    fn test_export_waterfall_tiles_empty_project() {
        let project = LuminoProject::new("Empty");
        let dir = tempdir().expect("临时目录应创建成功");
        let meta = export_waterfall_tiles(&project, dir.path(), "empty_hash", 128)
            .expect("空项目应返回默认元数据");
        assert_eq!(meta.cache_hash, "empty_hash");
        assert_eq!(meta.key_count, 128);
        assert_eq!(meta.measures_per_group, 4);
    }
}
