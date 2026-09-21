use super::*;

#[test]
fn dump_zone_params() {
    // 大体积 SF2（`test-file/sf2/test.sf2`）被 `.gitignore` 排除，CI 无此文件：
    // 缺失时跳过而非失败，保证 `cargo test --workspace` 在干净检出下全绿。
    let candidates = [
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/test.sf2"),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../test-file/sf2/test.sf2"),
    ];
    let path = candidates.iter().find(|p| p.exists());
    let Some(path) = path else {
        eprintln!("skip dump_zone_params: 未找到 test.sf2 fixture（CI 预期跳过）");
        return;
    };
    let sf = SoundFont::load(path, 0, 0, true, 48_000).expect("测试 SF2 应可加载");
    println!("samples: {}", sf.sample_count());
    let ids = sf.zones_at(60, 100);
    println!("zones at (60,100): {:?}", ids);
    for &id in ids.iter().take(4) {
        let z = sf.zone(id);
        println!(
            "zone {}: sample_id={} channels={} volume={:.6} pan={:.4} speed={:.6} cutoff={:?} res_db={} loop={:?} off={} end={} env(start={},delay={:.6},attack={:.6},hold={:.6},decay={:.6},sustain={:.4},release={:.6})",
            id,
            z.sample_id,
            z.channels,
            z.volume,
            z.pan,
            z.speed_mult,
            z.cutoff,
            z.resonance_db,
            z.loop_mode,
            z.offset,
            z.sample_end,
            z.envelope.start_percent,
            z.envelope.delay,
            z.envelope.attack,
            z.envelope.hold,
            z.envelope.decay,
            z.envelope.sustain_percent,
            z.envelope.release
        );
    }
}
