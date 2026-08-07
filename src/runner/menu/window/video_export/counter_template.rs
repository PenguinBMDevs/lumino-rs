//! 计数器模式模板渲染（占位符替换）
//!
//! 占位符全集参考 Zenith-MIDI NoteCountRender 的 `Render.cs`：
//!
//! | 占位符 | 含义 |
//! |--------|------|
//! | `{bpm}` | 当前 BPM |
//! | `{nc}` `{nr}` `{tn}` | 已开始 / 剩余 / 总音符数 |
//! | `{nps}` `{mnps}` | 每秒音符数及峰值 |
//! | `{plph}` `{mplph}` | 复音数及峰值 |
//! | `{currsec}` `{currtime}` `{cmiltime}` `{cfrtime}` | 当前秒 / mm:ss / mm:ss.fff / mm:ss;帧 |
//! | `{totalsec}` `{totaltime}` `{tmiltime}` `{tfrtime}` | 总时长（同上四种格式） |
//! | `{remsec}` `{remtime}` `{rmiltime}` `{rfrtime}` | 剩余时长（同上四种格式） |
//! | `{currticks}` `{totalticks}` `{remticks}` | 当前 / 总 / 剩余 tick |
//! | `{currbars}` `{totalbars}` `{rembars}` | 当前 / 总 / 剩余小节 |
//! | `{ppq}` `{tsn}` `{tsd}` | 分辨率 / 拍号 |
//! | `{avgnps}` | 平均每秒音符数 |
//! | `{currframes}` `{totalframes}` `{remframes}` | 当前 / 总 / 剩余帧 |
//! | `{notep}` `{tickp}` `{timep}` | 音符 / tick / 时间进度百分比 |

use lumino_midi_loader::MidiDocument;

use super::counter_format::{
    fmt_mmss, fmt_mmss_fff, format_float, format_int, format_percent, use_comma,
};
use super::counter_stats::{
    CounterRenderConfig, CounterStats, current_bpm, current_time_signature, ticks_to_seconds,
};

/// 模板渲染所需的全部上下文值。
pub(super) struct TemplateContext<'a> {
    pub config: &'a CounterRenderConfig,
    pub stats: &'a CounterStats,
    pub document: &'a MidiDocument,
    pub tick: u32,
    pub ppq: u32,
    pub fps: u32,
    /// 总时长（秒）
    pub duration_secs: f64,
}

/// 执行模板占位符替换（用于画面文本与 CSV 行）。
pub(super) fn render_template(ctx: &TemplateContext<'_>) -> String {
    let cfg = ctx.config;
    let st = ctx.stats;
    let doc = ctx.document;
    let tick = ctx.tick;
    let ppq = ctx.ppq;
    let fps = ctx.fps;
    let duration = ctx.duration_secs;
    let use_comma = use_comma(cfg.separator);
    let zeroes = cfg.padding_zeroes;

    let total_notes = doc.notes.iter().map(|t| t.len() as u64).sum::<u64>();
    let remaining = total_notes.saturating_sub(st.note_count);

    let bpm = current_bpm(&doc.tempo_changes, tick);
    let (tsn, tsd) = current_time_signature(&doc.time_signatures, tick);

    let total_ticks = doc.total_ticks;
    let time_secs = ticks_to_seconds(tick, &doc.tempo_changes, ppq);
    let rem_secs = (duration - time_secs).max(0.0);

    // 小节数（按当前拍号计算每小节 tick 数）
    let bar_divide = {
        let beat_ticks = ppq as f64 * 4.0 / tsd as f64;
        beat_ticks * tsn as f64
    };
    let bar_divide = if bar_divide > 0.0 { bar_divide } else { 1.0 };
    let curr_bar = (tick as f64 / bar_divide).floor() as u64 + 1;
    let total_bars = (total_ticks as f64 / bar_divide).floor() as u64;
    let max_bar = if curr_bar > total_bars {
        total_bars
    } else {
        curr_bar
    };

    let total_frames = (duration * fps as f64).ceil() as u64;
    let curr_frames = st.frames;
    let rem_frames = total_frames.saturating_sub(curr_frames);

    let avg_nps = if duration > 0.0 {
        total_notes as f64 / duration
    } else {
        0.0
    };

    let bpm_str = format_float(bpm, cfg.bpm_int_pad, cfg.bpm_dec_pad, use_comma, zeroes);
    let nc = format_int(st.note_count, cfg.note_count_pad, use_comma, zeroes);
    let nr = format_int(remaining, cfg.note_count_pad, use_comma, zeroes);
    let tn = format_int(total_notes, cfg.note_count_pad, use_comma, zeroes);
    let nps = format_int(st.nps, cfg.nps_pad, use_comma, zeroes);
    let mnps = format_int(st.max_nps, cfg.nps_pad, use_comma, zeroes);
    let plph = format_int(st.polyphony, cfg.polyphony_pad, use_comma, zeroes);
    let mplph = format_int(st.max_polyphony, cfg.polyphony_pad, use_comma, zeroes);
    let ticks = format_int(tick as u64, cfg.ticks_pad, use_comma, zeroes);
    let tticks = format_int(total_ticks as u64, cfg.ticks_pad, use_comma, zeroes);
    let rticks = format_int(
        total_ticks.saturating_sub(tick) as u64,
        cfg.ticks_pad,
        use_comma,
        zeroes,
    );
    let bars = format_int(max_bar, cfg.bars_pad, use_comma, zeroes);
    let tbars = format_int(total_bars, cfg.bars_pad, use_comma, zeroes);
    let rbars = format_int(
        total_bars.saturating_sub(max_bar),
        cfg.bars_pad,
        use_comma,
        zeroes,
    );
    let frames = format_int(curr_frames, cfg.frames_pad, use_comma, zeroes);
    let tframes = format_int(total_frames, cfg.frames_pad, use_comma, zeroes);
    let rframes = format_int(rem_frames, cfg.frames_pad, use_comma, zeroes);

    let currsec = format!("{time_secs:.1}");
    let totalsec = format!("{duration:.1}");
    let remsec = format!("{rem_secs:.1}");
    let avgnps = format!("{avg_nps:.2}");
    let notep = format_percent(st.note_count as f64, total_notes as f64);
    let tickp = format_percent(tick as f64, total_ticks as f64);
    let timep = format_percent(time_secs, duration);
    let cfrtime = format!("{};{}", fmt_mmss(time_secs), curr_frames % fps as u64);
    let tfrtime = format!("{};{}", fmt_mmss(duration), total_frames % fps as u64);
    let rfrtime = format!("{};{}", fmt_mmss(rem_secs), rem_frames % fps as u64);

    let mut out = cfg.text.clone();
    for (key, val) in [
        ("{bpm}", bpm_str.as_str()),
        ("{nc}", nc.as_str()),
        ("{nr}", nr.as_str()),
        ("{tn}", tn.as_str()),
        ("{nps}", nps.as_str()),
        ("{mnps}", mnps.as_str()),
        ("{plph}", plph.as_str()),
        ("{mplph}", mplph.as_str()),
        ("{currsec}", currsec.as_str()),
        ("{currtime}", fmt_mmss(time_secs).as_str()),
        ("{cmiltime}", fmt_mmss_fff(time_secs).as_str()),
        ("{cfrtime}", cfrtime.as_str()),
        ("{totalsec}", totalsec.as_str()),
        ("{totaltime}", fmt_mmss(duration).as_str()),
        ("{tmiltime}", fmt_mmss_fff(duration).as_str()),
        ("{tfrtime}", tfrtime.as_str()),
        ("{remsec}", remsec.as_str()),
        ("{remtime}", fmt_mmss(rem_secs).as_str()),
        ("{rmiltime}", fmt_mmss_fff(rem_secs).as_str()),
        ("{rfrtime}", rfrtime.as_str()),
        ("{currticks}", ticks.as_str()),
        ("{totalticks}", tticks.as_str()),
        ("{remticks}", rticks.as_str()),
        ("{currbars}", bars.as_str()),
        ("{totalbars}", tbars.as_str()),
        ("{rembars}", rbars.as_str()),
        ("{ppq}", ppq.to_string().as_str()),
        ("{tsn}", tsn.to_string().as_str()),
        ("{tsd}", tsd.to_string().as_str()),
        ("{avgnps}", avgnps.as_str()),
        ("{currframes}", frames.as_str()),
        ("{totalframes}", tframes.as_str()),
        ("{remframes}", rframes.as_str()),
        ("{notep}", notep.as_str()),
        ("{tickp}", tickp.as_str()),
        ("{timep}", timep.as_str()),
    ] {
        out = out.replace(key, val);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_event::window::video::{CounterAlignment, CounterSeparator};
    use lumino_midi_loader::{NoteEvent, TrackManager};

    fn test_config() -> CounterRenderConfig {
        CounterRenderConfig {
            text: String::new(),
            alignment: CounterAlignment::TopLeft,
            font_size: 40,
            font: lumino_event::window::video::CounterFont::Bitmap,
            separator: CounterSeparator::Comma,
            padding_zeroes: true,
            bpm_int_pad: 3,
            bpm_dec_pad: 2,
            note_count_pad: 5,
            polyphony_pad: 3,
            nps_pad: 3,
            ticks_pad: 5,
            bars_pad: 3,
            frames_pad: 5,
            save_csv: false,
            csv_output: std::path::PathBuf::new(),
            csv_format: String::new(),
        }
    }

    fn make_doc() -> MidiDocument {
        let notes = vec![(0u32, 480u32, 60u8), (480, 960, 62), (960, 1920, 64)];
        let mut list: Vec<NoteEvent> = notes
            .iter()
            .map(|&(s, e, k)| NoteEvent::new(s, e, k, 100, 0))
            .collect();
        list.sort_unstable_by_key(|n| n.start_tick);
        MidiDocument {
            notes: vec![lumino_midi_loader::ChunkedList::from_sorted(list)],
            tempo_changes: vec![(0, 120.0)],
            time_signatures: vec![(0, 4, 4)],
            key_signatures: vec![(0, 0, false)],
            control_events: lumino_midi_loader::ChunkedList::new(),
            lyrics: vec![],
            markers: vec![],
            sys_ex: vec![],
            track_names: vec![Some("T1".into())],
            total_ticks: 1920,
            track_count: 1,
            tracks: TrackManager::new(1),
            division: 480,
            track_ports: vec![],
            track_max_end_ticks: vec![],
        }
    }

    /// 模板替换：Zenith 默认模板关键占位符全部替换
    #[test]
    fn test_render_template_default() {
        let doc = make_doc();
        let mut config = test_config();
        config.text = "Notes: {nc} / {tn}\nBPM: {bpm}\nNPS: {nps}\nPPQ: {ppq}\nPolyphony: {plph}\nTime: {currtime}".to_string();

        let mut stats = CounterStats::default();
        stats.reset(&doc);
        // tick=960：已开始 2 个音符，'64' 从 960 开始也计入（<=960）→ 2 个
        stats.advance(&doc, 960, 60);

        let ctx = TemplateContext {
            config: &config,
            stats: &stats,
            document: &doc,
            tick: 960,
            ppq: 480,
            fps: 60,
            duration_secs: 4.0,
        };
        let out = render_template(&ctx);
        assert!(out.contains("Notes: 00,003 / 00,003"), "out={out}");
        assert!(out.contains("BPM: 120.00"), "out={out}");
        assert!(out.contains("NPS: 003"), "out={out}");
        assert!(out.contains("PPQ: 480"), "out={out}");
        assert!(out.contains("Polyphony: 001"), "out={out}"); // 64 音从 960 开始，960<end=1920 → 发声
        assert!(out.contains("Time: 00:01"), "out={out}");
        assert!(!out.contains('{'), "不应残留占位符: out={out}");
    }

    /// 未知占位符保持原样（用户模板错误时可见，不 panic）
    #[test]
    fn test_render_template_unknown_placeholder_kept() {
        let doc = make_doc();
        let mut config = test_config();
        config.text = "X {unknown} {nc}".to_string();
        let mut stats = CounterStats::default();
        stats.reset(&doc);
        stats.advance(&doc, 0, 60);
        let ctx = TemplateContext {
            config: &config,
            stats: &stats,
            document: &doc,
            tick: 0,
            ppq: 480,
            fps: 60,
            duration_secs: 4.0,
        };
        let out = render_template(&ctx);
        assert!(out.contains("{unknown}"), "未知占位符应保留");
        assert!(out.contains("00,001"), "已知占位符应替换");
    }

    /// 空文档（0 音符、0 时长）不 panic
    #[test]
    fn test_render_template_empty_doc() {
        let doc = MidiDocument {
            notes: vec![],
            tempo_changes: vec![(0, 120.0)],
            time_signatures: vec![(0, 4, 4)],
            key_signatures: vec![(0, 0, false)],
            control_events: lumino_midi_loader::ChunkedList::new(),
            lyrics: vec![],
            markers: vec![],
            sys_ex: vec![],
            track_names: vec![],
            total_ticks: 0,
            track_count: 0,
            tracks: TrackManager::new(0),
            division: 480,
            track_ports: vec![],
            track_max_end_ticks: vec![],
        };
        let config = test_config();
        let mut stats = CounterStats::default();
        stats.reset(&doc);
        stats.advance(&doc, 0, 60);
        let ctx = TemplateContext {
            config: &config,
            stats: &stats,
            document: &doc,
            tick: 0,
            ppq: 480,
            fps: 60,
            duration_secs: 0.0,
        };
        let _ = render_template(&ctx); // 不 panic
    }
}
