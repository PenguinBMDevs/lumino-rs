//! 更新计划纯函数验收：每条语义分支一条用例。
//!
//! 本文件只放共享 fixture 与子模块声明；用例按主题拆分到 `plan/tests/*`，
//! 保证每个测试文件 < 400 行（硬门槛，按真实行数计）。

use std::collections::HashSet;

use lumino_cpu_onion::CanvasCamera;
use lumino_editor_state::{CpuOnionChange, CpuOnionDirty};
use lumino_midi_model::NoteEvent;

use super::{CpuOnionIndex, CpuOnionPlan, CpuOnionRedraw, PlanInput, plan_update};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 240;

fn camera() -> CanvasCamera {
    CanvasCamera {
        width: WIDTH,
        height: HEIGHT,
        scroll_x: 0.0,
        scroll_y: 0.0,
        zoom_x: 1.0,
        zoom_y: 20.0,
        key_count: 128,
    }
}

/// 基准输入：轨 0 当前、轨 2 静音、有帧有索引、代次变化。
fn base<'a>(
    camera: &'a CanvasCamera,
    muted: &'a [bool],
    dirty: Option<&'a CpuOnionDirty>,
    tracks: Option<&'a HashSet<usize>>,
) -> PlanInput<'a> {
    PlanInput {
        frame_ready: true,
        index_ready: true,
        layout_changed: false,
        gen_changed: true,
        stale_needs_repair: false,
        dirty,
        dirty_tracks: tracks,
        current_track: 0,
        muted,
        camera,
        frame_size: (WIDTH, HEIGHT),
    }
}

fn change(track: usize, old: Option<NoteEvent>, new: Option<NoteEvent>) -> CpuOnionChange {
    CpuOnionChange { track, old, new }
}

fn dirty_with(changes: Vec<CpuOnionChange>) -> CpuOnionDirty {
    CpuOnionDirty {
        full: false,
        changes,
    }
}

fn tracks(ids: &[usize]) -> HashSet<usize> {
    ids.iter().copied().collect()
}

/// 断言「整帧 + 整索引」。
fn assert_full_rebuild(input: PlanInput<'_>) {
    assert_eq!(
        plan_update(input),
        CpuOnionPlan {
            redraw: Some(CpuOnionRedraw::Full),
            index: CpuOnionIndex::Rebuild,
        }
    );
}

/// 冷启动 / 帧缺失 / full / 来源未知（整帧兜底路径）。
#[path = "tests/full.rs"]
mod full;

/// 布局变化、切轨、当前轨直接编辑、复用（主路径）。
#[path = "tests/current.rs"]
mod current;

/// 洋葱轨脏区 / 整帧兜底 / 静音轨。
#[path = "tests/onion.rs"]
mod onion;

/// 变更聚合 / 矩形语义。
#[path = "tests/changes.rs"]
mod changes;

/// 脏矩形换算（越界裁剪 / 退化相机 / 全在画布外 / 面积升级）。
#[path = "tests/rects.rs"]
mod rects;

/// 总览密度 LOD 的重绘放大（Step 7）。
#[path = "tests/overview.rs"]
mod overview;
