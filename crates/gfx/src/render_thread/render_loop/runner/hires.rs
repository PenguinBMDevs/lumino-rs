use std::sync::{Arc, Mutex};

use crate::{HiResConfig, HiResRenderer};

use super::super::super::commands::ControlCommand;
use super::context::{HiResGenerateContext, RenderContext};
use super::types::{HiResMeta, HiResStreamMsg};

pub(crate) mod common;
pub(crate) mod dirty;
pub(crate) mod generate;
pub(crate) mod regen;
pub(crate) mod stream;
pub(crate) mod video;
pub(crate) mod viewport;

// Re-export public functions for external callers
pub(crate) use common::push_onion_progress;
pub(crate) use generate::handle_dispose_hires;
pub(crate) use stream::drain_hires_stream;
pub(crate) use video::{upload_hires_video_tiles, upload_hires_video_tiles_command};
pub(crate) use viewport::update_hires_viewport;

/// 处理高精度洋葱皮控制命令（分发器，各命令逻辑在独立模块中）
pub(super) fn handle_hires_control(
    cmd: ControlCommand,
    ctx: &RenderContext,
    hires_result_tx: &std::sync::mpsc::SyncSender<HiResStreamMsg>,
    onion_progress: &Arc<Mutex<Vec<(String, f32)>>>,
    hires_renderer: &mut Option<HiResRenderer>,
    hires_meta: &mut Option<HiResMeta>,
    hires_config: &mut Option<HiResConfig>,
) {
    match cmd {
        ControlCommand::GenerateHiResOnionSkin {
            notes,
            ppq,
            key_count,
            total_ticks,
            config,
            midi_hash,
        } => generate::handle_generate_hires(HiResGenerateContext {
            ctx,
            notes,
            ppq,
            key_count,
            total_ticks,
            config,
            midi_hash,
            hires_result_tx,
            onion_progress,
            hires_renderer,
            hires_meta,
            hires_config,
        }),
        ControlCommand::DisposeHiResOnionSkin => {
            generate::handle_dispose_hires(hires_renderer, hires_meta, hires_config, onion_progress)
        }
        ControlCommand::RegenerateHiResTrack(params) => regen::handle_regenerate_hires_track(
            params,
            ctx,
            hires_result_tx,
            hires_renderer,
            hires_meta,
            hires_config,
        ),
        ControlCommand::ShowHiResDirtyOverlay(params) => dirty::handle_show_dirty_overlay(
            params,
            ctx,
            hires_renderer,
            hires_meta,
            hires_config,
            onion_progress,
        ),
        _ => {}
    }
}
