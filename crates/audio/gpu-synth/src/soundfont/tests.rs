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

fn rich_sf2() -> Option<std::path::PathBuf> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../test-file/bench/rich.sf2");
    path.exists().then_some(path)
}

/// REF-LGS-013：CPU 采样字节 = 原生样本 + 重采样缓存；缺失夹具时跳过。
///
/// REF-LGS-014：按 `Arc` 指针去重——目标率与原生率相同时缓存直接复用原生
/// 采样，不得把同一份数据算两遍。
#[test]
fn memory_bytes_counts_native_and_resampled_cache() {
    let Some(path) = rich_sf2() else {
        eprintln!("skip memory_bytes_counts_native_and_resampled_cache: 未找到 rich.sf2");
        return;
    };
    let mut sf = SoundFont::load(&path, 0, 0, true, 48_000).expect("rich.sf2 应可加载");
    let baseline = sf.memory_bytes();
    assert!(baseline > 0, "加载后应统计到原生采样字节");
    assert!(sf.sample_count() > 0, "rich.sf2 应含采样");

    // 同率：缓存项是原生采样的同一份 Arc → 字节数不变。
    let _ = sf.resample(0, 48_000);
    assert_eq!(
        sf.memory_bytes(),
        baseline,
        "同率重采样只应缓存 Arc，不得重复计字节"
    );

    // 异率：真正的新数据 → 计入。
    let _ = sf.resample(0, 24_000);
    assert!(
        sf.memory_bytes() > baseline,
        "重采样缓存应计入 CPU 采样字节（baseline={baseline}）"
    );
}

/// REF-DATA-001：同率重采样是「别名」——缓存命中但不计字节，也不触发淘汰。
#[test]
fn same_rate_resample_aliases_native_and_is_free() {
    let Some(path) = rich_sf2() else {
        eprintln!("skip same_rate_resample_aliases_native_and_is_free: 未找到 rich.sf2");
        return;
    };
    let mut sf = SoundFont::load(&path, 0, 0, true, 48_000).expect("rich.sf2 应可加载");
    let native = sf.memory_bytes_split().0;
    assert!(native > 0);

    sf.cache_resampled(0, 48_000, sf.sample_data(0).clone());
    let (n, r) = sf.memory_bytes_split();
    assert_eq!(n, native, "原生字节不应变化");
    assert_eq!(r, 0, "同率缓存与原生共享 Arc，不应计入重采样字节");
}

/// REF-DATA-001：异率缓存超过上限时按 LRU 淘汰，字节数保持有界。
#[test]
fn resample_cache_evicts_when_over_budget() {
    let Some(path) = rich_sf2() else {
        eprintln!("skip resample_cache_evicts_when_over_budget: 未找到 rich.sf2");
        return;
    };
    let cap = super::accessors::RESAMPLE_CACHE_MAX_BYTES;
    let mut sf = SoundFont::load(&path, 0, 0, true, 48_000).expect("rich.sf2 应可加载");

    // 每份 128K f32 = 512 KiB；测试上限 1 MiB → 插入 3 份后必然淘汰 1 份。
    let chunk = 128 * 1024;
    for i in 0..3u32 {
        let data: Arc<[f32]> = Arc::from(vec![0.0f32; chunk]);
        sf.cache_resampled(0, 11_000 + i, data);
    }
    let (_, resample) = sf.memory_bytes_split();
    assert!(
        resample <= cap,
        "重采样缓存必须保持有界：{resample} > {cap}"
    );
    assert_eq!(
        sf.cache().map.len(),
        2,
        "三份 512 KiB 在 1 MiB 上限下应只剩两份"
    );
}

/// REF-LGS-014：同一文件第二次 `load_cached` 复用同一个解析核心（采样数据
/// 与重采样缓存都只有一份）。
#[test]
fn load_cached_shares_core_between_instances() {
    let Some(path) = rich_sf2() else {
        eprintln!("skip load_cached_shares_core_between_instances: 未找到 rich.sf2");
        return;
    };
    let a = SoundFont::load_cached(&path, 0, 0, true, 48_000).expect("首次加载");
    let b = SoundFont::load_cached(&path, 0, 0, true, 48_000).expect("二次加载");
    assert!(
        a.shares_core_with(&b),
        "同文件同参数的第二次加载必须复用解析结果"
    );
    assert!(
        Arc::ptr_eq(a.sample_data(0), b.sample_data(0)),
        "采样数据必须是同一份 Arc"
    );
    assert_eq!(
        a.memory_bytes(),
        b.memory_bytes(),
        "两份句柄统计的是同一份数据"
    );
}

/// REF-LGS-014：源文件变化（mtime 或大小）必须 miss，不能返回旧解析。
#[test]
fn load_cached_invalidates_when_file_changes() {
    use std::time::{Duration, SystemTime};

    let Some(src) = rich_sf2() else {
        eprintln!("skip load_cached_invalidates_when_file_changes: 未找到 rich.sf2");
        return;
    };
    let dir = std::env::temp_dir().join(format!(
        "lumino_sf_cache_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("创建临时目录");
    let copy = dir.join("copy.sf2");
    std::fs::copy(&src, &copy).expect("复制 rich.sf2");

    let first = SoundFont::load_cached(&copy, 0, 0, true, 48_000).expect("首次加载");
    let same = SoundFont::load_cached(&copy, 0, 0, true, 48_000).expect("命中加载");
    assert!(first.shares_core_with(&same), "未改动时应命中");

    // 只改 mtime（内容仍是合法 SF2）——缓存键必须失效。
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&copy)
        .expect("打开副本");
    file.set_modified(SystemTime::now() + Duration::from_secs(120))
        .expect("设置 mtime");
    drop(file);

    let after = SoundFont::load_cached(&copy, 0, 0, true, 48_000).expect("失效后重新解析");
    assert!(
        !first.shares_core_with(&after),
        "mtime 变化后必须重新解析，不能复用旧核心"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
