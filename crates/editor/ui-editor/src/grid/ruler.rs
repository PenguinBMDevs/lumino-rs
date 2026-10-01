//! 时间轴标尺绘制

use super::loop_range::LoopRange;
use super::theme::ThemeExt;
use crate::Editor;
use iced_core::{Point, Rectangle, Size, alignment};
use iced_widget::canvas::{Frame, Geometry, Path, Stroke, Text};
use lumino_ui_core::Renderer;
use lumino_ui_core::constants::editor::MEASURE_NUMBER_FONT_SIZE;

/// 绘制时间轴标尺到 Geometry（用于 Canvas 绘制）
pub fn draw_to_geometry(
    editor: &Editor,
    renderer: &Renderer,
    bounds: Rectangle,
    theme: &lumino_ui_core::Theme,
) -> Geometry<Renderer> {
    let mut frame = Frame::new(renderer, bounds.size());
    draw(editor, &mut frame, bounds, theme);
    frame.into_geometry()
}

/// 绘制时间轴标尺（小节号显示区域）
pub fn draw(
    editor: &Editor,
    frame: &mut Frame<Renderer>,
    bounds: Rectangle,
    theme: &lumino_ui_core::Theme,
) {
    let view = &editor.editor_state.view;
    let ppq = view.ppq as f32;
    let keyboard_width = view.keyboard_width;
    let ruler_height = view.ruler_height;

    let time_signatures = editor.editor_state.data.time_signatures.as_slice();
    let start_tick = view.scroll_x / view.zoom_x;
    let end_tick = (view.scroll_x + bounds.width - keyboard_width) / view.zoom_x;

    // 绘制标尺背景
    let ruler_bg_color = theme.ruler_background_color();
    let ruler_rect = Rectangle::new(
        Point::new(keyboard_width, 0.0),
        Size::new(bounds.width - keyboard_width, ruler_height),
    );
    let ruler_path = Path::rectangle(ruler_rect.position(), ruler_rect.size());
    frame.fill(&ruler_path, ruler_bg_color);

    // 绘制标尺边框
    let border_stroke = Stroke::default()
        .with_width(1.0)
        .with_color(theme.border_color());
    frame.stroke(&ruler_path, border_stroke);

    let text_color = theme.text_color();

    // 绘制小节号和刻度线（按拍号变化分段）
    let mut measure_iter = MeasureIterator::new(time_signatures, ppq, start_tick.max(0.0));
    while let Some((measure_tick, measure_number)) = measure_iter.next() {
        if measure_tick > end_tick {
            break;
        }

        let screen_x = (measure_tick * view.zoom_x) - view.scroll_x + keyboard_width;

        if screen_x >= keyboard_width && screen_x <= bounds.width {
            // 绘制小节号文本
            let measure_text = Text {
                content: measure_number.to_string(),
                position: Point::new(screen_x + 4.0, 4.0),
                max_width: bounds.width - keyboard_width,
                line_height: iced_core::text::LineHeight::Relative(1.0),
                size: iced_core::Pixels(MEASURE_NUMBER_FONT_SIZE),
                color: text_color,
                font: lumino_ui_core::font::ui_font(),
                align_x: alignment::Horizontal::Left.into(),
                align_y: alignment::Vertical::Top,
                shaping: iced_core::text::Shaping::Basic,
            };
            frame.fill_text(measure_text);

            // 绘制刻度线
            let tick_stroke = Stroke::default()
                .with_width(1.0)
                .with_color(theme.border_color());
            let tick_path = Path::line(
                Point::new(screen_x, 0.0),
                Point::new(screen_x, ruler_height),
            );
            frame.stroke(&tick_path, tick_stroke);
        }
    }

    // 绘制循环区域（在刻度线之上）
    if let Some(loop_range) = &editor.loop_range
        && loop_range.enabled()
    {
        let loop_view = LoopRangeViewParams {
            keyboard_width,
            scroll_x: view.scroll_x,
            zoom_x: view.zoom_x,
            ruler_height,
            bounds_width: bounds.width,
        };
        draw_loop_range(frame, loop_range, &loop_view, theme);
    }
}

/// 循环区域颜色常量
const LOOP_FILL_ALPHA: f32 = 0.25;
const LOOP_BORDER_ALPHA: f32 = 0.7;
const LOOP_HANDLE_WIDTH: f32 = 6.0;
const LOOP_HANDLE_HEIGHT: f32 = 16.0;

/// 绘制循环区域高亮和标记点的视图参数
struct LoopRangeViewParams {
    keyboard_width: f32,
    scroll_x: f32,
    zoom_x: f32,
    ruler_height: f32,
    bounds_width: f32,
}

/// 绘制循环区域高亮和标记点
fn draw_loop_range(
    frame: &mut Frame<Renderer>,
    loop_range: &LoopRange,
    view: &LoopRangeViewParams,
    theme: &lumino_ui_core::Theme,
) {
    let Some((start_x, end_x)) =
        loop_range.to_screen_coords(view.keyboard_width, view.scroll_x, view.zoom_x)
    else {
        return;
    };

    // 如果循环区域完全不在可视范围内，不绘制
    if end_x < view.keyboard_width || start_x > view.bounds_width {
        return;
    }

    let visible_start = start_x.max(view.keyboard_width);
    let visible_end = end_x.min(view.bounds_width);

    if visible_end <= visible_start {
        return;
    }

    // 获取主题主色调作为循环区域颜色
    let palette = theme.extended_palette();
    let primary_color = palette.primary.weak.color;

    // 绘制半透明背景填充
    let fill_color = iced_core::Color {
        r: primary_color.r,
        g: primary_color.g,
        b: primary_color.b,
        a: LOOP_FILL_ALPHA,
    };

    let loop_rect = Rectangle::new(
        Point::new(visible_start, 2.0),
        Size::new(visible_end - visible_start, view.ruler_height - 4.0),
    );

    let fill_path = Path::rectangle(loop_rect.position(), loop_rect.size());
    frame.fill(&fill_path, fill_color);

    // 绘制边框
    let border_color = iced_core::Color {
        r: primary_color.r,
        g: primary_color.g,
        b: primary_color.b,
        a: LOOP_BORDER_ALPHA,
    };
    let border_stroke = Stroke::default().with_width(2.0).with_color(border_color);
    frame.stroke(&fill_path, border_stroke);

    // 绘制起始手柄（左侧三角形/竖条）
    if start_x >= view.keyboard_width && start_x <= view.bounds_width {
        draw_handle(frame, start_x, view.ruler_height, true, border_color);
    }

    // 绘制结束手柄（右侧三角形/竖条）
    if end_x >= view.keyboard_width && end_x <= view.bounds_width {
        draw_handle(frame, end_x, view.ruler_height, false, border_color);
    }
}

/// 绘制拖拽手柄
fn draw_handle(
    frame: &mut Frame<Renderer>,
    x: f32,
    ruler_height: f32,
    _is_start: bool,
    color: iced_core::Color,
) {
    let handle_y = (ruler_height - LOOP_HANDLE_HEIGHT) / 2.0;

    // 竖直矩形手柄
    let handle_rect = Rectangle::new(
        Point::new(x - LOOP_HANDLE_WIDTH / 2.0, handle_y),
        Size::new(LOOP_HANDLE_WIDTH, LOOP_HANDLE_HEIGHT),
    );
    let handle_path = Path::rectangle(handle_rect.position(), handle_rect.size());

    // 手柄使用更深的颜色
    let handle_fill = iced_core::Color {
        r: color.r,
        g: color.g,
        b: color.b,
        a: 0.9,
    };
    frame.fill(&handle_path, handle_fill);

    // 手柄边框
    let handle_stroke = Stroke::default()
        .with_width(1.0)
        .with_color(iced_core::Color {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 0.5,
        });
    frame.stroke(&handle_path, handle_stroke);
}

/// 计算 tick 位置所在拍号段（空列表时回退到 4/4）
fn time_signature_at(tick: f32, time_signatures: &[(u32, u8, u8)]) -> (u8, u8) {
    let mut active = (4_u8, 4_u8);
    for &(ts_tick, num, den) in time_signatures {
        if tick >= ts_tick as f32 {
            active = (num, den);
        } else {
            break;
        }
    }
    active
}

/// 计算给定拍号下的每小节 tick 数
fn ticks_per_measure(ppq: f32, numerator: u8, denominator: u8) -> f32 {
    let beat_ticks = ppq * 4.0 / denominator.max(1) as f32;
    beat_ticks * numerator.max(1) as f32
}

/// 小节边界迭代器，按拍号变化分段生成小节起始 tick 与编号
struct MeasureIterator<'a> {
    time_signatures: &'a [(u32, u8, u8)],
    ppq: f32,
    current_tick: f32,
    measure_number: u32,
    ts_index: usize,
    measure_ticks: f32,
}

impl<'a> MeasureIterator<'a> {
    fn new(time_signatures: &'a [(u32, u8, u8)], ppq: f32, start_tick: f32) -> Self {
        let mut iter = Self::at_origin(time_signatures, ppq);
        iter.advance_to(start_tick);
        iter
    }

    /// 从 tick 0 / 小节 1 起始的迭代器状态（不含起点定位）。
    fn at_origin(time_signatures: &'a [(u32, u8, u8)], ppq: f32) -> Self {
        let (num, den) = time_signature_at(0.0, time_signatures);
        Self {
            time_signatures,
            ppq,
            current_tick: 0.0,
            measure_number: 1,
            ts_index: 0,
            measure_ticks: ticks_per_measure(ppq, num, den),
        }
    }

    /// 第 `index` 个拍号段的起始 tick。
    ///
    /// **段 0 的起点恒视为 0**——与旧实现一致：`ts_index = 0` 时迭代器从 tick 0
    /// 起步、按 `ts[0]` 的拍号走格，`ts[0].tick` 只通过 `time_signature_at(0.0)`
    /// 参与「初始拍号」的选取，不参与起点定位。正常 MIDI 的首个拍号恒在 tick 0，
    /// 该口径对常规输入无差异，但它是逐位等价的前提。
    fn segment_start(&self, index: usize) -> f32 {
        if index == 0 {
            return 0.0;
        }
        self.time_signatures
            .get(index)
            .map_or(0.0, |&(tick, _, _)| tick as f32)
    }

    /// 容纳 `target_tick` 的拍号段下标（最后一个起始 tick ≤ target 的段）。
    fn segment_index_at(&self, target_tick: f32) -> usize {
        let mut index = 0;
        for (i, &(tick, _, _)) in self.time_signatures.iter().enumerate() {
            if (tick as f32) <= target_tick {
                index = i;
            } else {
                break;
            }
        }
        index
    }

    /// 段 `index` 内的小节数（按 `step()` 的边界语义：跨段时该段不足一小节也计 1）。
    fn measures_in_segment(&self, index: usize) -> Option<u32> {
        let start = self.segment_start(index);
        let end = self
            .time_signatures
            .get(index + 1)
            .map(|&(tick, _, _)| tick as f32)?;
        let (num, den) = time_signature_at(start, self.time_signatures);
        let ticks = ticks_per_measure(self.ppq, num, den).max(f32::EPSILON);
        Some((((end - start) / ticks).ceil() as u32).max(1))
    }

    /// 定位到 `target_tick`（或其后的第一个小节边界）。
    ///
    /// 复杂度 **O(段数)**：先定位拍号段，再按「段内小节长度恒定」直接算出
    /// 小节号与边界 tick。
    ///
    /// 旧实现是从 tick 0 逐小节 `step()` 推进，成本 O(起点之前的小节数)——
    /// 横向滚动时标尺缓存每帧失效（`Editor::set_scroll_x` → `invalidate_caches(RULER)`），
    /// 于是**每帧**都要重跑一遍；tick 跨度大的工程（黑乐谱/长曲）会线性劣化，
    /// 病态文件（巨大 tick 跨度）可到毫秒级每帧。
    fn advance_to(&mut self, target_tick: f32) {
        let seg = self.segment_index_at(target_tick);
        let start = self.segment_start(seg);
        let (num, den) = time_signature_at(start, self.time_signatures);
        let ticks = ticks_per_measure(self.ppq, num, den).max(f32::EPSILON);

        // 段内已走过的小节数（含之前所有段）
        let mut number = 1u32;
        for i in 0..seg {
            number += self.measures_in_segment(i).unwrap_or(1);
        }

        // 段内直接跳：k = ceil((target - start) / ticks)，与旧 step 循环
        // 「current_tick < target 时继续步进」的落点一致（target 落在格线上时
        // 不越界，落在格线之间时落到下一条格线）。
        let steps = ((target_tick - start) / ticks).ceil().max(0.0) as u32;
        self.ts_index = seg;
        self.measure_ticks = ticks;

        match self.measures_in_segment(seg) {
            // 落在本段最后（可能不满的）小节 → 与 step() 的跨段跳转一致：
            // 直接落到下一段起点，并把段号推进一格
            Some(in_segment) if steps >= in_segment => {
                let end = self.segment_start(seg + 1);
                self.current_tick = end;
                self.measure_number = number + in_segment;
                if let Some(&(_, n, d)) = self.time_signatures.get(seg + 1) {
                    self.ts_index = seg + 1;
                    self.measure_ticks = ticks_per_measure(self.ppq, n, d);
                }
            }
            _ => {
                self.current_tick = start + steps as f32 * ticks;
                self.measure_number = number + steps;
            }
        }
    }

    fn step(&mut self) {
        let next_measure_tick = self.current_tick + self.measure_ticks;
        if let Some((next_ts_tick, _, _)) = self.time_signatures.get(self.ts_index + 1) {
            let next_ts_tick = *next_ts_tick as f32;
            if next_measure_tick >= next_ts_tick && self.current_tick < next_ts_tick {
                self.ts_index += 1;
                let (num, den) = time_signature_at(next_ts_tick, self.time_signatures);
                self.measure_ticks = ticks_per_measure(self.ppq, num, den);
                self.current_tick = next_ts_tick;
                self.measure_number += 1;
                return;
            }
        }
        self.current_tick = next_measure_tick;
        self.measure_number += 1;
    }

    fn next(&mut self) -> Option<(f32, u32)> {
        let result = (self.current_tick, self.measure_number);
        self.step();
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_signature_at_empty_defaults_to_4_4() {
        assert_eq!(time_signature_at(100.0, &[]), (4, 4));
    }

    #[test]
    fn test_time_signature_at_returns_active_signature() {
        let signatures = [(0, 3, 4), (1920, 4, 4)];
        assert_eq!(time_signature_at(0.0, &signatures), (3, 4));
        assert_eq!(time_signature_at(1919.0, &signatures), (3, 4));
        assert_eq!(time_signature_at(1920.0, &signatures), (4, 4));
    }

    #[test]
    fn test_measure_iterator_4_4() {
        // ppq = 480, 4/4 -> 1920 ticks/measure
        let mut iter = MeasureIterator::new(&[(0, 4, 4)], 480.0, 0.0);
        assert_eq!(iter.next(), Some((0.0, 1)));
        assert_eq!(iter.next(), Some((1920.0, 2)));
        assert_eq!(iter.next(), Some((3840.0, 3)));
    }

    #[test]
    fn test_measure_iterator_advance_to_start() {
        // ppq = 480, 4/4, start at tick 4000
        let mut iter = MeasureIterator::new(&[(0, 4, 4)], 480.0, 4000.0);
        // measure 3 spans [3840, 5760), so the first visible boundary is measure 4 at 5760
        assert_eq!(iter.next(), Some((5760.0, 4)));
        assert_eq!(iter.next(), Some((7680.0, 5)));
    }

    #[test]
    fn test_measure_iterator_time_signature_change() {
        // 4/4 for one measure (1920 ticks), then 3/4 (1440 ticks/measure)
        let signatures = [(0, 4, 4), (1920, 3, 4)];
        let mut iter = MeasureIterator::new(&signatures, 480.0, 0.0);
        assert_eq!(iter.next(), Some((0.0, 1)));
        assert_eq!(iter.next(), Some((1920.0, 2)));
        assert_eq!(iter.next(), Some((1920.0 + 1440.0, 3)));
        assert_eq!(iter.next(), Some((1920.0 + 2880.0, 4)));
    }

    #[test]
    fn test_ticks_per_measure_different_denominators() {
        // ppq = 480, 6/8 -> beat = 480 * 4 / 8 = 240, measure = 240 * 6 = 1440
        assert!((ticks_per_measure(480.0, 6, 8) - 1440.0).abs() < f32::EPSILON);
    }

    /// 等价性测试用例：(拍号表, ppq)
    type MeasureCase = (Vec<(u32, u8, u8)>, f32);

    /// **等价性（蓝军自证）**：O(段数) 定位必须与旧的「从 tick 0 逐小节步进」
    /// 逐位一致——包括落点 tick、小节号，以及其后 8 个小节边界的全部输出。
    ///
    /// 覆盖：单拍号、空拍号表、首个拍号不在 tick 0、跨段边界前后 1 tick、
    /// 段内不满一小节的尾部、多段切换、不同 ppq。
    #[test]
    fn test_measure_iterator_advance_to_matches_stepping_reference() {
        let cases: Vec<MeasureCase> = vec![
            (vec![(0, 4, 4)], 480.0),
            (vec![], 480.0),
            (vec![(100, 4, 4)], 480.0),
            (vec![(0, 4, 4), (1920, 3, 4)], 480.0),
            (vec![(0, 3, 4), (1920, 4, 4), (3360, 6, 8)], 480.0),
            (
                vec![(0, 4, 4), (500, 5, 4), (900, 7, 8), (4000, 2, 4)],
                960.0,
            ),
        ];

        for (signatures, ppq) in cases {
            // 探测点：均匀采样 + 每个段起点及其前后各 1 tick（边界最易错）
            let mut probes: Vec<f32> = (0..600).map(|i| i as f32 * 7.0).collect();
            for &(tick, _, _) in &signatures {
                let t = tick as f32;
                probes.extend([t - 1.0, t, t + 1.0]);
            }
            probes.push(1.0e6);

            for &target in &probes {
                let target = target.max(0.0);
                // 参考实现：旧算法原样（从 0 逐小节 step）
                let mut reference = MeasureIterator::at_origin(&signatures, ppq);
                while reference.current_tick < target {
                    reference.step();
                }
                let mut fast = MeasureIterator::new(&signatures, ppq, target);

                assert_eq!(
                    (fast.current_tick, fast.measure_number),
                    (reference.current_tick, reference.measure_number),
                    "sigs={signatures:?} ppq={ppq} target={target}：定位结果不一致"
                );
                for k in 0..8 {
                    assert_eq!(
                        fast.next(),
                        reference.next(),
                        "sigs={signatures:?} ppq={ppq} target={target}：第 {k} 个后续边界不一致"
                    );
                }
            }
        }
    }

    /// 病态 tick 跨度下不得退化为 O(小节数)：旧实现定位到 1e9 tick 需要约
    /// 52 万次 `step()`（每帧标尺重建都要重跑），新实现为常数量级。
    #[test]
    fn test_measure_iterator_large_tick_span_is_constant_time() {
        let signatures = [(0u32, 4u8, 4u8)];
        let target = 1.0e9_f32;

        let started = std::time::Instant::now();
        let mut iter = MeasureIterator::new(&signatures, 480.0, target);
        let elapsed = started.elapsed();

        // 闭式落点与实现同式，锁住「不越格线」的语义（target 恰在格线上时不前进）
        let ticks = ticks_per_measure(480.0, 4, 4);
        let steps = (target / ticks).ceil() as u32;
        assert_eq!(iter.measure_number, 1 + steps);
        assert_eq!(iter.current_tick, steps as f32 * ticks);
        assert!(iter.next().is_some());

        assert!(
            elapsed < std::time::Duration::from_millis(200),
            "大 tick 跨度定位耗时 {elapsed:?}，疑似退化为逐小节步进"
        );
    }
}
