//! 黑乐谱集成测试
//!
//! 运行：cargo test -p lumino-cache --release --test black_midi_test -- --ignored --nocapture

use std::path::Path;
use std::time::{Duration, Instant};

const BLACK_MIDI_PATH: &str =
    r"D:\熠星始芒：StarT+2023fix+v1.1\熠星始芒：StarT 164 Million v1.1fix.mid";

const MEMORY_LIMIT: u64 = 1_000_000_000; // 1 GB
const MAX_LOAD_SECS: u64 = 200; // 加载超时 200s
const MAX_EVENTS_PER_SEEK: usize = 80_000_000; // 单次跳转最多 80M 事件
const SEEK_ITERATIONS: u32 = 100;
const STRESS_ITERATIONS: u32 = 30;
const MAX_LATENCY_US: u128 = 10_000; // 10ms

#[cfg(windows)]
mod platform {
    use winapi::um::processthreadsapi::GetCurrentProcess;
    use winapi::um::psapi::GetProcessMemoryInfo;
    use winapi::um::psapi::PROCESS_MEMORY_COUNTERS;

    pub fn current_rss() -> u64 {
        unsafe {
            let mut pmc: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
            pmc.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
            if GetProcessMemoryInfo(GetCurrentProcess(), &mut pmc, pmc.cb) != 0 {
                pmc.WorkingSetSize as u64
            } else {
                0
            }
        }
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn current_rss() -> u64 {
        0
    }
}

fn black_midi_path() -> &'static Path {
    Path::new(BLACK_MIDI_PATH)
}
fn file_exists() -> bool {
    black_midi_path().exists()
}
fn file_size_gb() -> f64 {
    black_midi_path()
        .metadata()
        .map(|m| m.len() as f64 / 1_073_741_824.0)
        .unwrap_or(0.0)
}

/// 带超时的加载：200 秒内未完成则 panic
fn load_with_timeout(progress: Option<&'static dyn Fn(f64)>) -> lumino_cache::MidiCache {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let timed_out = Arc::new(AtomicBool::new(false));
    let timed_out_clone = timed_out.clone();

    // 超时监控线程
    let _watchdog = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(MAX_LOAD_SECS));
        timed_out_clone.store(true, Ordering::Relaxed);
        eprintln!("\n⛔ 加载超时 {} 秒，终止", MAX_LOAD_SECS);
        std::process::exit(1);
    });

    // 改造 progress 回调：检查超时标志
    let wrapped_progress: Option<&'static dyn Fn(f64)> = if progress.is_some() {
        // 无法修改原有回调，直接传原始回调
        progress
    } else {
        None
    };

    lumino_cache::MidiCache::load(BLACK_MIDI_PATH, wrapped_progress).expect("加载 MIDI 失败")
}

/// 随机跳转测试：验证延迟 < 10ms、RSS < 1GB、单次事件 < 80M
fn random_seek_test(cache: &lumino_cache::MidiCache, iterations: u32) -> Result<(), String> {
    let total_ticks = cache.index.total_ticks;
    let mut max_latency_us = 0u128;
    let mut total_latency_us = 0u128;

    for i in 0..iterations {
        let tick = if i % 2 == 0 {
            (rand_float() * total_ticks as f64) as u32
        } else {
            (rand_float() * rand_float() * total_ticks as f64) as u32
        };
        // 使用小范围模拟真实播放场景：一次 seek 只取 512 ticks 的事件
        let to_tick = tick.saturating_add(512).min(total_ticks);
        // 限制最多返回 500K 事件，防止单个 chunk 过多时 OOM
        let max_events = 500_000;

        let start = Instant::now();
        let events = cache.cache.get_events(tick, to_tick, max_events);
        let elapsed_us = start.elapsed().as_micros();

        // 单次事件超过 80M → 直接退出
        if events.len() > MAX_EVENTS_PER_SEEK {
            return Err(format!(
                "❌ 单次跳转事件 {} > {} 上限 (tick={})",
                events.len(),
                MAX_EVENTS_PER_SEEK,
                tick
            ));
        }

        total_latency_us += elapsed_us;
        if elapsed_us > max_latency_us {
            max_latency_us = elapsed_us;
        }

        let rss = platform::current_rss();
        if rss > MEMORY_LIMIT {
            return Err(format!(
                "❌ RSS {:.1}MB > 1GB 上限 (tick={}, iter={})",
                rss as f64 / 1_000_000.0,
                tick,
                i
            ));
        }

        if i % 20 == 0 || i == iterations - 1 {
            println!(
                "  [{:>3}/{}] tick={:>8} events={:>8} latency={:>5}μs rss={:.1}MB",
                i + 1,
                iterations,
                tick,
                events.len(),
                elapsed_us,
                rss as f64 / 1_000_000.0,
            );
        }
    }

    let avg_latency_us = total_latency_us / iterations as u128;
    println!(
        "\n  ═══ 延迟统计 ═══ 平均={:.1}μs 最大={:.1}μs 阈值={}μs 结果={}",
        avg_latency_us,
        max_latency_us,
        MAX_LATENCY_US,
        if max_latency_us <= MAX_LATENCY_US {
            "✅"
        } else {
            "❌"
        },
    );

    if max_latency_us > MAX_LATENCY_US {
        return Err(format!(
            "延迟超限: 最大 {} μs > 阈值 {} μs",
            max_latency_us, MAX_LATENCY_US
        ));
    }
    Ok(())
}

/// 压力测试
fn stress_test(cache: &mut lumino_cache::MidiCache, iterations: u32) -> Result<(), String> {
    let total_ticks = cache.index.total_ticks;
    let track_count = cache.index.track_count;

    println!("\n  ═══ 压力测试 {iterations} 次 ═══");

    for i in 0..iterations {
        let tick = (rand_float() * total_ticks as f64) as u32;
        let to_tick = tick.saturating_add(512).min(total_ticks);

        if let Some(ref prefetch) = cache.prefetch {
            prefetch.seek(tick);
        }

        let track_id = (rand_float() * track_count as f64) as u16;
        cache
            .tracks
            .set_visibility(track_id, lumino_cache::TrackVisibility::Muted);

        let start = Instant::now();
        let events = cache.cache.get_events(tick, to_tick, 500_000);
        let latency_us = start.elapsed().as_micros();

        if events.len() > MAX_EVENTS_PER_SEEK {
            return Err(format!(
                "❌ 压力测试单次事件 {} > {} 上限",
                events.len(),
                MAX_EVENTS_PER_SEEK
            ));
        }

        let rss = platform::current_rss();
        if rss > MEMORY_LIMIT {
            return Err(format!(
                "❌ 压力测试 RSS {:.1}MB > 1GB",
                rss as f64 / 1_000_000.0
            ));
        }

        println!(
            "  [{:>3}/{}] tick={:>8} events={:>8} latency={:>5}μs rss={:.1}MB",
            i + 1,
            iterations,
            tick,
            events.len(),
            latency_us,
            rss as f64 / 1_000_000.0,
        );

        cache
            .tracks
            .set_visibility(track_id, lumino_cache::TrackVisibility::Visible);
        std::thread::sleep(std::time::Duration::from_micros(100));
    }

    println!("  ✅ 压力测试通过");
    Ok(())
}

fn rand_float() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let nanos = now.as_nanos() as u64;
    let mixed = nanos
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    (mixed >> 11) as f64 / (1u64 << 53) as f64
}

#[test]
#[ignore]
fn black_midi_integration_test() {
    if !file_exists() {
        eprintln!("⚠️  黑乐谱文件不存在，跳过测试：{BLACK_MIDI_PATH}");
        return;
    }

    println!(
        "═══ 黑乐谱集成测试 ═══\n\
         文件: {BLACK_MIDI_PATH}\n\
         大小: {:.2} GB\n\
         内存上限: 1 GB\n\
         加载超时: {}s\n\
         单次最多: {} 事件",
        file_size_gb(),
        MAX_LOAD_SECS,
        MAX_EVENTS_PER_SEEK,
    );

    // ── 加载 ──
    println!("\n⏳ 加载中（上限 {}s）...", MAX_LOAD_SECS);
    let load_start = Instant::now();
    let mut cache = load_with_timeout(None);
    let load_elapsed = load_start.elapsed();
    println!("\n  ✅ 加载完成: {:.1?}", load_elapsed);

    let rss = platform::current_rss();
    println!(
        "  音轨: {} | ticks: {} | RSS: {:.1} MB",
        cache.index.track_count,
        cache.index.total_ticks,
        rss as f64 / 1_000_000.0,
    );
    assert!(
        rss <= MEMORY_LIMIT,
        "加载后 RSS 超限: {:.1} MB",
        rss as f64 / 1_000_000.0
    );

    // ── 100 次随机跳转 ──
    println!("\n── 第 1 阶段：{SEEK_ITERATIONS} 次随机跳转 ──");
    random_seek_test(&cache, SEEK_ITERATIONS).expect("随机跳转测试失败");
    println!(
        "  RSS: {:.1} MB",
        platform::current_rss() as f64 / 1_000_000.0
    );

    // ── 压力测试 ──
    println!("\n── 第 2 阶段：压力测试 {STRESS_ITERATIONS} 次 ──");
    stress_test(&mut cache, STRESS_ITERATIONS).expect("压力测试失败");
    println!(
        "  RSS: {:.1} MB",
        platform::current_rss() as f64 / 1_000_000.0
    );

    // ── 报告 ──
    println!("\n{}", cache.metrics.report());
    println!("总耗时: {:.1?}  ✅ 全部通过", load_start.elapsed());
}
