//! 统计 MIDI 文件每轨音符数 + 深度平局索引饱和分析（黑乐谱验证工具）
//!
//! 用途：验证「轨内确定性微深度裁决」在高索引下的饱和行为——
//! 主音轨层（base = 2^-17）的索引预算约 6.29M，洋葱皮层约 2.09M；
//! 超过预算的索引会饱和到同一深度，同轨叠音将退化为平局（后画者胜）。
//!
//! 用法：
//! `cargo run -p lumino-midi-loader --example count_track_notes -- <path.mid>`

use std::path::PathBuf;

use lumino_midi_loader::loader::load_parsed_midi;

/// 主音轨层可用的最大索引（tail 于 2^-17 基深度的一个 ulp 预算减半）。
const MAIN_TRACK_INDEX_CAP: u64 = 6_291_455;
/// 洋葱皮层可用的最大索引。
const ONION_TRACK_INDEX_CAP: u64 = 2_097_151;

#[tokio::main]
async fn main() {
    let path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .expect("用法: count_track_notes <midi path>");

    let parsed = load_parsed_midi(path.clone(), None)
        .await
        .expect("MIDI 加载失败");
    let doc = parsed.document.as_ref().expect("document 缺失");

    println!("文件: {}", path.display());
    println!(
        "音轨数: {} | 总音符: {} | 时长 tick: {}",
        doc.track_count(),
        parsed.info.total_notes,
        doc.total_ticks()
    );
    println!(
        "预算: 主音轨层 cap = {MAIN_TRACK_INDEX_CAP} | 洋葱皮层 cap = {ONION_TRACK_INDEX_CAP}"
    );
    println!("------");

    // GPU 缓冲布局 = 全轨段按 track_id 顺序紧凑排列（段间无间隙）
    let mut offset: u64 = 0;
    let mut any_over_main = 0u64;
    let mut any_over_onion = 0u64;
    for t in 0..doc.track_count() {
        let n = doc.track_note_count(t as u16);
        let start = offset;
        let end = offset + n;
        offset = end;

        // 饱和分析：段内全局索引超过 cap 的部分（cap = 全局索引）
        let over_main = end.saturating_sub(MAIN_TRACK_INDEX_CAP.max(start));
        let over_onion = end.saturating_sub(ONION_TRACK_INDEX_CAP.max(start));
        any_over_main += over_main;
        any_over_onion += over_onion;

        let flags = if over_main > 0 || over_onion > 0 {
            "  ← 饱和"
        } else {
            ""
        };
        println!(
            "track {t:>3}: notes={n:>10} buffer=[{start:>10},{end:>10}) 超主轨cap={over_main:>9} 超洋葱cap={over_onion:>9}{flags}"
        );
    }
    println!("------");
    println!("buffer 总实例: {offset}");
    println!("若任一轨为当前主音轨：超主轨 cap 音符合计 {any_over_main}");
    println!("若任一轨为洋葱皮：超洋葱 cap 音符合计 {any_over_onion}");
}
