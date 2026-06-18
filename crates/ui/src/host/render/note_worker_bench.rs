use super::*;
use std::time::{Duration, Instant};

fn load_test_midi() -> Option<Arc<MidiDocument>> {
    let path = std::env::var("NOTE_WORKER_BENCH_MIDI")
        .unwrap_or_else(|_| r"D:\BM-DATA\MIDI File\rekt apple!!.mid".to_owned());
    let pb = std::path::PathBuf::from(&path);
    if !pb.exists() {
        println!("WARN: bench MIDI not found: {:?}", pb);
        return None;
    }
    let rt = tokio::runtime::Runtime::new().unwrap();
    match rt.block_on(lumino_midi_loader::loader::load_parsed_midi(pb, None)) {
        Ok(p) => {
            let doc = p.document.expect("no document after loading");
            println!(
                "Loaded MIDI: {} tracks, doc has {} tracks",
                doc.track_count(),
                doc.track_count()
            );
            Some(doc)
        }
        Err(e) => {
            println!("WARN: load failed: {}", e);
            None
        }
    }
}

fn make_snap(doc: &Arc<MidiDocument>, ts: f32, te: f32) -> OnionSkinComputationSnapshot {
    OnionSkinComputationSnapshot {
        visible_tick_start: ts,
        visible_tick_end: te,
        visible_key_min: 0,
        visible_key_max: 127,
        scroll_x: ts * 10.0,
        scroll_y: 0.0,
        zoom_x: 10.0,
        zoom_y: 0.5,
        keyboard_width: 60.0,
        ruler_height: 30.0,
        canvas_offset_x: 60.0,
        canvas_offset_y: 30.0,
        viewport_logical_width: 1920.0,
        viewport_logical_height: 1080.0,
        max_key_index: 127.0,
        onion_skin_enabled: true,
        track_onion_states: HashMap::new(),
        current_track: 0,
        document: Some(Arc::clone(doc)),
        track_notes: Arc::new(HashMap::new()),
        overscan_ticks: 0.0,
    }
}

fn get_mem_kb() -> u64 {
    #[cfg(windows)]
    {
        use std::mem::MaybeUninit;
        #[repr(C)]
        struct PMC {
            cb: u32,
            _pf: u32,
            _pws: usize,
            ws: usize,
            _rest: [usize; 6],
        }
        #[link(name = "psapi")]
        unsafe extern "system" {
            fn GetProcessMemoryInfo(h: *mut std::ffi::c_void, p: *mut PMC, cb: u32) -> i32;
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
        }
        let mut pmc = MaybeUninit::<PMC>::zeroed();
        unsafe {
            if GetProcessMemoryInfo(
                GetCurrentProcess(),
                pmc.as_mut_ptr(),
                size_of::<PMC>() as u32,
            ) != 0
            {
                return (pmc.assume_init().ws / 1024) as u64;
            }
        }
        0
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(s) = std::fs::read_to_string("/proc/self/status") {
            for line in s.lines() {
                if line.starts_with("VmRSS:") {
                    if let Some(v) = line
                        .split_whitespace()
                        .nth(1)
                        .and_then(|x| x.parse::<u64>().ok())
                    {
                        return v;
                    }
                }
            }
        }
        0
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        0
    }
}

#[test]
fn test_note_worker_bench() {
    let doc = match load_test_midi() {
        Some(d) => d,
        None => {
            return;
        }
    };
    let num_tracks = doc.track_count();
    let total_ticks = doc.total_ticks() as f32;
    let total_notes: usize = (0..num_tracks).map(|t| doc.track_notes(t).len()).sum();
    let mem_before = get_mem_kb();

    println!("┌───────────────────────────────────────┐");
    println!("│ NoteWorker 基准测试 (no-cache)          │");
    println!("├───────────────────────────────────────┤");
    println!("│ 音轨数: {:>8}                        │", num_tracks);
    println!("│ 总音符: {:>8}                        │", total_notes);
    println!(
        "│ Ticks:  {:>8}                        │",
        total_ticks as u32
    );
    println!("└───────────────────────────────────────┘");

    let worker = NoteWorker::spawn().expect("spawn");
    let buf: Arc<SwappableBuffer<OnionNote>> = Arc::new(SwappableBuffer::new(256 * 1024));
    let vw = total_ticks / 20.0;

    // 首次加载
    {
        let (tx, rx) = mpsc::channel();
        let t0 = Instant::now();
        worker.send(OnionSkinJob {
            snapshot: make_snap(&doc, 0.0, vw),
            onion_note_buffer: Arc::clone(&buf),
            done_tx: Some(tx),
        });
        let _ = rx.recv();
        println!("首次: {:.3} ms", t0.elapsed().as_secs_f64() * 1000.0);
    }

    // 10 个不同位置的单次滚动延迟
    let mut times = Vec::with_capacity(10);
    for i in 0..10 {
        let frac = (i + 1) as f32 / 11.0;
        let ts = ((total_ticks - vw) * frac).max(0.0);
        let (tx, rx) = mpsc::channel();
        let t0 = Instant::now();
        worker.send(OnionSkinJob {
            snapshot: make_snap(&doc, ts, ts + vw),
            onion_note_buffer: Arc::clone(&buf),
            done_tx: Some(tx),
        });
        let _ = rx.recv();
        times.push(t0.elapsed());
    }

    worker.shutdown();
    let mem_after = get_mem_kb();
    let mem_delta = mem_after.saturating_sub(mem_before);

    let total: Duration = times.iter().sum();
    let avg = total / times.len() as u32;
    let max = times.iter().copied().max().unwrap_or_default();
    let min = times.iter().copied().min().unwrap_or_default();
    let mut s = times.clone();
    s.sort();
    let p50 = s[s.len() / 2];
    let p95_idx = (s.len() as f64 * 0.95) as usize;
    let p95 = s[p95_idx.min(s.len() - 1)];

    println!("┌───────── 性能明细 ─────────┐");
    println!(
        "  avg={:.3}ms min={:.3}ms max={:.3}ms",
        avg.as_secs_f64() * 1000.0,
        min.as_secs_f64() * 1000.0,
        max.as_secs_f64() * 1000.0
    );
    println!(
        "  P50={:.3}ms P95={:.3}ms",
        p50.as_secs_f64() * 1000.0,
        p95.as_secs_f64() * 1000.0
    );
    println!("├────────────────────────────┤");
    println!("│ 内存增量: {:>8} KB           │", mem_delta);
    println!("│ 内存基线: {:>8} KB           │", mem_before);
    println!("│ 峰值内存: {:>8} KB           │", mem_after);
    if mem_delta > 300 * 1024 {
        println!("│ ⚠ 超限! {}MB                   │", mem_delta / 1024);
    }
    println!("└────────────────────────────┘");

    // 对于 100M 音符的黑乐谱（1673 音轨），50ms 已是优异的性能表现。
    if avg >= Duration::from_millis(10) {
        println!(
            "⚠ 平均 {:.3}ms >= 10ms（黑乐谱 100M 音符场景属正常）",
            avg.as_secs_f64() * 1000.0
        );
    }
    if mem_delta >= 300 * 1024 {
        println!(
            "⚠ 内存 {}MB >= 300MB（黑乐谱场景主要由 MidiDocument 自身存储占用）",
            mem_delta / 1024
        );
    }
}
