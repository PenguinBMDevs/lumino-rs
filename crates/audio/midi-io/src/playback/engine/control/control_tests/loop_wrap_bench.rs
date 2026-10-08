//! PREF-006 A2 基准：回绕扫描墙钟（`#[ignore]`，需 `PREF006_MIDI` 环境变量）
//!
//! 用法：
//! `PREF006_MIDI=<path.mid> cargo test -p lumino-midi-io --release -- --ignored loop_wrap_scan_bench --nocapture`
//!
//! 口径：`reset_cursors_to` 与旧 `handle_loop_wrap` 的悬挂音符扫描逐字同源
//! （同一 O(N) 全扫）；A2 上线后新增「缓存构建一次 / 稳态应用」两段计时对比。

use super::*;

#[test]
#[ignore = "基准：需 PREF006_MIDI 环境变量指向真实 MIDI 文件"]
fn loop_wrap_scan_bench() {
    let Ok(path) = std::env::var("PREF006_MIDI") else {
        eprintln!("skip：未设置 PREF006_MIDI");
        return;
    };
    let doc = Arc::new(
        MidiDocument::from_notes_file(std::path::Path::new(&path), None).expect("加载 MIDI 失败"),
    );
    let end = doc.tracks_max_end_tick() as f32;
    let loop_start = (end - 1920.0).max(0.0);
    println!(
        "file={path}\ntracks={} loop_start={loop_start} end={end}",
        doc.track_count()
    );
    let playback = Arc::new(Mutex::new(Playback::new(480)));
    let mut engine = PlaybackEngine::new(playback);
    engine.set_document(Arc::clone(&doc), 0);

    // 基线：旧回绕同口径（reset_cursors_to 的 O(N) 扫描）
    let mut samples = Vec::new();
    for _ in 0..5 {
        let t = std::time::Instant::now();
        engine.reset_cursors_to(loop_start);
        samples.push(t.elapsed());
    }
    samples.sort();
    println!("baseline reset_cursors_to: first={:?}", samples[0]);
    println!("baseline reset_cursors_to: median={:?}", samples[2]);
    println!("baseline reset_cursors_to: worst={:?}", samples[4]);

    // A2 缓存：首次构建一次 + 稳态应用（100 次取中位/最差）
    let t = std::time::Instant::now();
    engine.ensure_loop_wrap_cache(loop_start);
    println!("cache build (first wrap): {:?}", t.elapsed());
    let mut apply_samples = Vec::new();
    for _ in 0..100 {
        let t = std::time::Instant::now();
        engine.apply_loop_wrap_cache();
        apply_samples.push(t.elapsed());
    }
    apply_samples.sort();
    println!("cache apply x100: median={:?}", apply_samples[50]);
    println!("cache apply x100: worst={:?}", apply_samples[99]);

    // 回绕的另一半成本：当前轨事件队列重建（A1 目标，未优化）
    let mut rebuild_samples = Vec::new();
    for _ in 0..5 {
        let t = std::time::Instant::now();
        engine.rebuild_queue_from_current_track(Some(loop_start));
        rebuild_samples.push(t.elapsed());
    }
    rebuild_samples.sort();
    println!(
        "baseline rebuild_current_queue: median={:?} worst={:?}",
        rebuild_samples[2], rebuild_samples[4]
    );
}
