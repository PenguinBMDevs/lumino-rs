//! REF-DATA-002 验证：同一 SF2 在 XSynth 栈与 LGS 栈**各自解析一次**。
//!
//! 本测试只证明「两套加载栈不共享解析结果」这一事实（#226 的根因），
//! 不声称运行时一定并发——是否并发由 App 的实时后端与导出后端组合决定。
//!
//! 夹具缺失时跳过（`test-file/` gitignored，CI 无此文件）。

use std::path::PathBuf;
use std::time::Instant;

use lumino_gpu_synth::SoundFont;
use lumino_gpu_synth::soundfont as lgs_sf;
use lumino_midi_io::soundfont_cache as xs_sf;
use xsynth_core::{AudioStreamParams, ChannelCount};

fn rich_sf2() -> Option<PathBuf> {
    // 允许用大音源复测（例如 Jasmine 3.4 GB）：REF_DATA_SF2=<path>
    if let Ok(p) = std::env::var("REF_DATA_SF2") {
        let path = PathBuf::from(p);
        // 相对路径按仓库根解析（测试进程 CWD 是 crate 目录）。
        let path = if path.is_absolute() {
            path
        } else {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../..")
                .join(path)
        };
        return path.exists().then_some(path);
    }
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../test-file/bench/rich.sf2");
    path.exists().then_some(path)
}

#[test]
fn cross_stack_parses_same_file_twice() {
    let Some(path) = rich_sf2() else {
        eprintln!("skip cross_stack_parses_same_file_twice: 未找到 rich.sf2");
        return;
    };

    xs_sf::clear_cache();
    xs_sf::reset_soundfont_cache_stats();
    lgs_sf::reset_soundfont_cache_stats();

    let t = Instant::now();
    let _xs_a =
        xs_sf::load_soundfont_cached(&path, AudioStreamParams::new(48_000, ChannelCount::Stereo))
            .expect("XSynth 首次加载");
    let xs_parse = t.elapsed();
    let t = Instant::now();
    let _xs_b =
        xs_sf::load_soundfont_cached(&path, AudioStreamParams::new(48_000, ChannelCount::Stereo))
            .expect("XSynth 命中");
    let xs_hit = t.elapsed();
    let (xs_parses, xs_hits) = xs_sf::soundfont_cache_stats();

    // 顺序测量，避免大音源（Jasmine 3.4 GB）两栈同时驻留导致峰值翻倍。
    drop(_xs_a);
    drop(_xs_b);
    xs_sf::clear_cache();

    let t = Instant::now();
    let _lgs_a = SoundFont::load_cached(&path, 0, 0, true, 48_000).expect("LGS 首次加载");
    let lgs_parse = t.elapsed();
    let t = Instant::now();
    let _lgs_b = SoundFont::load_cached(&path, 0, 0, true, 48_000).expect("LGS 命中");
    let lgs_hit = t.elapsed();
    let (lgs_parses, lgs_hits) = lgs_sf::soundfont_cache_stats();

    assert_eq!((xs_parses, xs_hits), (1, 1), "XSynth 栈同文件应只解析一次");
    assert_eq!((lgs_parses, lgs_hits), (1, 1), "LGS 栈同文件应只解析一次");
    assert_eq!(
        xs_parses + lgs_parses,
        2,
        "两套加载栈各自解析 → 同一文件在进程内解析两次（#226）"
    );

    println!("xs_parse={xs_parse:?} xs_hit={xs_hit:?} lgs_parse={lgs_parse:?} lgs_hit={lgs_hit:?}");
}
