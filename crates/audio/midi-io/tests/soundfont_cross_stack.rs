//! REF-DATA-002 验证：同一 SF2 在 XSynth 与 LGS 两栈**共享同一份解析结果**。
//!
//! 断言：两栈各加载两次时，presets 只被真正解析 **1 次**；XSynth 构建 1 次、
//! 命中 1 次；LGS core 构建 1 次、命中 1 次。夹具缺失时跳过。
//!
//! 大音源可经 `REF_DATA_SF2` 指定（相对路径按仓库根解析）。

use std::path::PathBuf;
use std::time::Instant;

use lumino_gpu_synth::SoundFont;
use lumino_gpu_synth::soundfont as lgs_sf;
use lumino_midi_io::soundfont_cache as xs_sf;
use xsynth_core::{AudioStreamParams, ChannelCount};

fn rich_sf2() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("REF_DATA_SF2") {
        let path = PathBuf::from(p);
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
fn cross_stack_shares_one_parse() {
    let Some(path) = rich_sf2() else {
        eprintln!("skip cross_stack_shares_one_parse: 未找到 rich.sf2");
        return;
    };

    xs_sf::clear_cache();
    xs_sf::reset_soundfont_cache_stats();
    lgs_sf::reset_soundfont_cache_stats();
    lgs_sf::reset_preset_cache_stats();

    let t = Instant::now();
    let _xs_a =
        xs_sf::load_soundfont_cached(&path, AudioStreamParams::new(48_000, ChannelCount::Stereo))
            .expect("XSynth 首次加载");
    let xs_build = t.elapsed();
    let t = Instant::now();
    let _xs_b =
        xs_sf::load_soundfont_cached(&path, AudioStreamParams::new(48_000, ChannelCount::Stereo))
            .expect("XSynth 命中");
    let xs_hit = t.elapsed();
    let (xs_builds, xs_hits) = xs_sf::soundfont_cache_stats();

    // 保持 XSynth 句柄存活：它与 LGS 共享 presets（采样 Arc），
    // 句柄释放会让 presets 缓存（Weak）回收，导致下一栈重新解析。
    let t = Instant::now();
    let _lgs_a = SoundFont::load_cached(&path, 0, 0, true, 48_000).expect("LGS 首次加载");
    let lgs_parse = t.elapsed();
    let t = Instant::now();
    let _lgs_b = SoundFont::load_cached(&path, 0, 0, true, 48_000).expect("LGS 命中");
    let lgs_hit = t.elapsed();
    let (lgs_parses, lgs_hits) = lgs_sf::soundfont_cache_stats();
    let (preset_parses, preset_hits) = lgs_sf::preset_cache_stats();

    assert_eq!(
        (xs_builds, xs_hits),
        (1, 1),
        "XSynth 栈应构建一次、命中一次"
    );
    assert_eq!(
        (lgs_parses, lgs_hits),
        (1, 1),
        "LGS core 应构建一次、命中一次"
    );
    assert_eq!(preset_parses, 1, "两栈共享 presets → 同一文件只解析一次");
    assert_eq!(preset_hits, 1, "LGS 构建应命中 XSynth 已解析的 presets");

    println!(
        "xs_build={xs_build:?} xs_hit={xs_hit:?} lgs_parse={lgs_parse:?} lgs_hit={lgs_hit:?} preset=({preset_parses},{preset_hits})"
    );
}
