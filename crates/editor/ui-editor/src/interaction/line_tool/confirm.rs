//! 曲线工具确认/取消：路径 + 填充区域 → 批量生成音符（√ / × 按钮）
//!
//! 生成规则（蜘蛛网 Spiderweb 式，**不使用"设定精度"**）：
//! 1. 每条路径按几何容差展平为折线（[`geom::flatten_path`]），再用
//!    [`paths::path_notes`] 逐音高行解析求出跨越边界；每个音高行一条音符，
//!    起点 = 进入该行的 tick、终点 = 下一条音符的起点 → 无缝、长度自然变化；
//!    随后按**全局绘制粗细**（`Editor::draw_thickness`）**沿线段法线**加粗
//!    （[`stroke_band::outline_notes`]：水平段纵向铺 N 行、竖直段横向加宽、拐角补齐
//!    外角方格——只按 key 向上平移对竖直段是"拉长"而不是加粗，那是已修的 BUG）；
//! 2. 颜料桶标记的封闭区域用 [`fill::fill_notes`]（与预览同源）逐音高行求
//!    **内部区间**，区间端点 = 闭环边与行边界的解析交点；开启切分档位时
//!    每个区间再按 x 分音符的全局网格切成多条音符；
//! 3. 两部分合并后同 tick 同 key 只留最长（[`paths::keep_longest`]），
//!    再按**最小长度下限**补齐过短的音符（默认 128 分音符，`paths::min_note_length_ticks`：
//!    竖直段只有 1 tick，实机缩放下不足 1px 等于没生成），
//!    写入当前音轨并使用 `CreateOp` 操作日志；
//! 4. 写入按规模分流（见 `interaction::batch_insert`）：小规模逐音符插入
//!    （保留 GPU 段内增量），超过 `BATCH_INSERT_THRESHOLD` 走批量归并——
//!    逐音符写入会为每个音符产生一条 `InsertAt` 渲染事件，万级音符时
//!    在 GPU 内造成数百 MB~GB 级实例搬移（见 `ui_fill_confirm_bench`）。
//!
//! 从 `line_tool.rs` 拆出（文件长度纪律）：交互处理与提交职责分离。

use super::{fill, geom, paths, stroke_band};
use crate::{Editor, Note};
use lumino_editor_state::{BezierAnchor, ShapeNote};
use lumino_note_core::history::CreateOp;
use std::collections::HashSet;

impl Editor {
    /// 一条路径的**预览音符块**（逻辑坐标 `(起 tick, 止 tick, key)`）。
    ///
    /// 与 [`Self::confirm_line_tool`] 的描边腿**同源**——同一个
    /// [`stroke_band::outline_blocks`]、同一组 zoom / 全局粗细 / 长度下限。画布预览
    /// 直接画这些块，于是"看到的宽度"就是"√ 后生成的宽度"：此前的预览是**居中**描边
    /// （宽度 = N × 单 key 高），而产物是**单侧向外**铺 `N-1` 层，两者差到用户报的
    /// "宽度显示 X2"。
    ///
    /// 逐路径独立生成（与下面的"逐路径音符归属"同一口径）：多条路径重叠时预览按各自
    /// 路径各画一份，写入侧由跨路径 `keep_longest` 收敛——重叠区域的**覆盖**两者相同，
    /// 仅"归谁"不同（与 [`Self::record_line_tool_paths`] 的既有说明一致）。
    pub(crate) fn path_outline_blocks(&self, path: &[BezierAnchor]) -> Vec<(f32, f32, u16)> {
        if path.len() < 2 {
            return Vec::new();
        }
        let px_per_tick = self.editor_state.view.zoom_x;
        let px_per_key = self.editor_state.view.zoom_y;
        let key_count = self.editor_state.view.key_count as i32;
        let min_ticks = paths::min_note_length_ticks(
            self.editor_state.line_tool.min_note_division,
            self.editor_state.view.ppq,
        );
        stroke_band::outline_blocks(
            &geom::flatten_path(path),
            self.draw_thickness(),
            px_per_tick,
            px_per_key,
            key_count,
            min_ticks,
        )
    }

    /// 确认全部路径与填充：按蜘蛛网式逐音高行算法批量生成音符（√ 按钮）。
    ///
    /// 成功后清空全部路径、填充与编辑历史；返回是否生成了音符。
    pub(crate) fn confirm_line_tool(&mut self) -> bool {
        // Conductor 音轨（track 0）禁止放置音符：整曲线工具不可用，
        // 与铅笔 finish_drawing、文字工具 confirm_text_tool 同源守卫
        if self.editor_state.data.current_track == 0 {
            tracing::debug!("曲线工具: Conductor 轨道禁止放置音符");
            return false;
        }
        let line_paths = self.editor_state.line_tool.paths.clone();
        let fill_marks = self.editor_state.line_tool.fill.clone();
        // 全局绘制粗细（纵向 key 层数）+ 视图像素尺度：加粗沿线段法线做，需要 zoom
        let thickness = self.draw_thickness();
        let zoom_x = self.editor_state.view.zoom_x;
        let zoom_y = self.editor_state.view.zoom_y;
        let key_count = self.editor_state.view.key_count as i32;

        // ① 路径轮廓 → 音符：几何容差展平后逐音高行解析求交（无网格量化），
        //    再按全局粗细**沿法线**加粗（水平段纵向、竖直段横向——见 `stroke_band`）
        let mut raw: Vec<paths::RawNote> = Vec::new();
        for path in &line_paths {
            if path.len() < 2 {
                continue;
            }
            let poly = geom::flatten_path(path);
            raw.extend(stroke_band::outline_notes(
                &poly, thickness, zoom_x, zoom_y, key_count,
            ));
        }

        // ② 颜料桶填充 → 逐音高行内部区间（与预览同源：fill::fill_notes）
        if !fill_marks.is_empty() {
            let fill_raw = fill::fill_notes(self);
            // 切分模式下填充音符优先：剔除与其 (start, key) 完全重合的轮廓音符。
            // keep_longest 按 (start, key) 分组保最长，若轮廓长音符与切分音符
            // 同起点同行，切分结果会被整条吞掉（用户看不到切分效果）。
            if self.editor_state.line_tool.fill_division.is_some() {
                raw.retain(|n| {
                    !fill_raw
                        .iter()
                        .any(|f| f.start == n.start && f.key == n.key)
                });
            }
            raw.extend(fill_raw);
        }

        // ③ 同 tick 同 key 只留最长（轮廓与填充大量重叠）+ 钳到合法 tick / key
        //    + 长度补齐到最小长度下限（默认 128 分音符，见 `paths::min_note_length_ticks`）
        let min_ticks = paths::min_note_length_ticks(
            self.editor_state.line_tool.min_note_division,
            self.editor_state.view.ppq,
        );
        let notes: Vec<(f32, u16, f32)> = paths::keep_longest(&raw)
            .into_iter()
            .filter(|n| n.key >= 0 && n.key < key_count)
            .map(|n| paths::to_note_triple(&n, min_ticks))
            .collect();
        if notes.is_empty() {
            return false;
        }

        // 逐路径音符归属（供「鼠标工具」登记图形对象）：每条路径独立按同规则生成，
        // 跨路径首占去重。路径互不重叠时与整体 keep_longest 结果逐项一致；
        // 重叠时仅归属可能落在先登记的路径上，音符本身（写入 payload）不受影响。
        let owner_track = self.editor_state.data.current_track;
        let mut per_path_notes: Vec<Vec<ShapeNote>> = Vec::with_capacity(line_paths.len());
        let mut claimed: HashSet<(i64, u16)> = HashSet::new();
        for path in &line_paths {
            if path.len() < 2 {
                per_path_notes.push(Vec::new());
                continue;
            }
            // 与写入 payload 同源（含全局粗细的**法线加粗** + 最小长度下限）：
            // 直接复用预览块入口 [`Self::path_outline_blocks`]，杜绝"预览一套、登记一套"
            let owned: Vec<ShapeNote> = self
                .path_outline_blocks(path)
                .into_iter()
                .filter(|(t, _, k)| claimed.insert((*t as i64, *k)))
                .map(|(start, end, key)| ShapeNote {
                    track: owner_track,
                    tick: start,
                    key,
                    length: end - start,
                })
                .collect();
            per_path_notes.push(owned);
        }

        let track = self.editor_state.data.current_track;
        let total = notes.len();
        let mut create_ops = Vec::with_capacity(total);
        if total <= super::super::BATCH_INSERT_THRESHOLD {
            // 小规模：逐音符插入（当前轨自动记录 GPU 段内增量事件）
            for (start, key, length) in notes {
                let note = Note::new(start, key, length);
                // 按值记录：redo 按值重插，undo 按值删除（删加语义，无 ID）
                if self
                    .editor_state
                    .data
                    .insert_note_with_id(track, note.clone())
                    .is_some()
                {
                    create_ops.push(CreateOp {
                        track_id: track as u32,
                        note: lumino_editor_state::note_to_event(note),
                    });
                }
            }
        } else {
            // 大规模：批量归并写入（单次 O(N+M)）。
            //
            // 逐音符插入会为每个音符记一条 `InsertAt` 事件，渲染侧逐条发
            // `NoteEvent::Insert` 并在 GPU 内搬移其后的全部实例——填充生成顺序
            // （按音高行、行内按 tick，跨行 tick 回绕）使每条插入都落在列表中
            // 部，Σtail 超线性增长：实测 13288 音符 = 13288 条消息 +
            // 6805 万实例搬移（≈1038 MB GPU 拷贝）+ 13288 次 submit，
            // 直接压垮渲染线程（见 `ui_fill_confirm_bench`）。批量只发一次
            // 主轨结构重建（TrackDelta）。
            let payload: Vec<Note> = notes
                .into_iter()
                .map(|(start, key, length)| Note::new(start, key, length))
                .collect();
            let before = self.editor_state.data.current_track_note_count();
            self.editor_state.data.batch_insert_notes_with_ids(&payload);
            if self.editor_state.data.current_track_note_count() == before {
                // 音轨不存在等异常：未写入任何音符（与逐音符路径「全部失败」同义）
                return false;
            }
            // 按值记录：redo 按值重插，undo 按值删除（删加语义，无 ID）
            create_ops.extend(payload.iter().map(|note| CreateOp {
                track_id: track as u32,
                note: lumino_editor_state::note_to_event(note.clone()),
            }));
        }
        if create_ops.is_empty() {
            return false;
        }

        // 批量创建操作日志（撤销/重做）+ 标记当前轨变化
        self.editor_state.data.history.push_note_create(create_ops);
        self.editor_state.data.mark_current_track_changed();
        // 登记图形对象供「鼠标工具」点选/移动/删除（须在清空路径之前读几何）
        self.record_line_tool_paths(per_path_notes);
        // 清空全部路径与历史并驱动渲染刷新
        self.editor_state.line_tool.reset();
        self.mark_notes_changed();
        true
    }

    /// 取消全部路径（× 按钮）
    pub(crate) fn cancel_line_tool(&mut self) {
        self.editor_state.line_tool.reset();
    }
}
