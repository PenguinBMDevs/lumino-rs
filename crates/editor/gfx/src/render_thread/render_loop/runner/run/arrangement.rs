use super::*;

use crate::ArrangementNoteUniform;

/// 预计算走带音符层所需的 GPU 数据（复用钢琴卷帘常驻 GPU 音符缓冲，零第二份显存）。
///
/// 输入：`params.arrangement_track_order`（侧栏音轨顺序，元素=文档音轨 id，索引=泳道序号）。
///
/// 输出（写入 `params`）：
/// - `arrangement_lane_index`：`lane_index[doc_track] = 泳道序号`（着色器按 doc track 索引）；
///   静音/隐藏轨置 `f32::MAX` 哨兵，GPU 裁剪着色器据此自动剔除其音符。
/// - `arrangement_note_uniform`：滚动/缩放/泳道高/画布偏移等走带专属 uniform。
///
/// 横向/纵向滚动只需更新 uniform（scroll.x / scroll.y），无需任何重建；GPU 裁剪
/// 计算着色器在每帧一次性完成可视范围剔除，彻底消除此前每帧 ~67ms 的 CPU 音符重建。
pub(super) fn prepare_arrangement_note_data(params: &mut RenderParams) {
    puffin::profile_scope!("arrangement::note_data");
    let track_order = &params.arrangement_track_order;
    let nt = track_order.len();
    if nt == 0 {
        params.arrangement_lane_index.clear();
        return;
    }

    // 1. lane_index[doc_track] = 泳道序号；静音/隐藏轨置哨兵值，GPU 裁剪自动剔除
    let max_doc = track_order.iter().cloned().max().unwrap_or(0) as usize;
    let mut lane_index = vec![0.0f32; max_doc.saturating_add(1).max(1)];
    for (lane, &doc) in track_order.iter().enumerate() {
        let visible = params
            .arrangement_track_visible
            .get(lane)
            .copied()
            .unwrap_or(true);
        lane_index[doc as usize] = if visible { lane as f32 } else { f32::MAX };
    }

    // 2. 走带专属 uniform（滚动/缩放/泳道高/画布偏移）
    // 泳道高度必须与 CPU 端 lane 背景（arrangement_instances.rs 的
    // `track_height * zoom_y`）完全一致，否则垂直缩放时音符与背景泳道失配。
    let au = &params.arrangement_uniform;
    let lh = au.track_height * au.zoom_y;
    params.arrangement_lane_index = lane_index;
    params.arrangement_note_uniform = ArrangementNoteUniform {
        scroll: au.scroll,
        zoom: [au.zoom, 1.0],
        viewport_size: au.viewport_size,
        canvas_offset: au.canvas_offset,
        lane_height: lh,
        note_height: 4.0,
        _pad: [0.0, 0.0],
    };
}
