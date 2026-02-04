//! MIDI Loader 测试程序
//! 
//! 简化版：仅加载 MIDI 文件并输出音符数量和 BPM 统计
//! 支持多文件并行加载

use lumino_midiloader::{LoadOptions, MmapMidiLoader, MmapReader};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    
    if args.len() < 2 {
        eprintln!("用法: midi_test <midi文件路径> [更多文件...]");
        std::process::exit(1);
    }

    // 收集有效的 MIDI 文件
    let files: Vec<PathBuf> = args[1..]
        .iter()
        .map(PathBuf::from)
        .filter(|p| {
            if !p.exists() {
                eprintln!("警告: 文件不存在: {}", p.display());
                return false;
            }
            let is_midi = p.extension()
                .map(|e| {
                    let ext = e.to_string_lossy().to_lowercase();
                    ext == "mid" || ext == "midi"
                })
                .unwrap_or(false);
            if !is_midi {
                eprintln!("警告: 不是 MIDI 文件: {}", p.display());
            }
            is_midi
        })
        .collect();

    if files.is_empty() {
        eprintln!("错误: 没有有效的 MIDI 文件可供加载");
        std::process::exit(1);
    }

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║              MIDI Loader 测试工具                            ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!("待加载文件数量: {}", files.len());
    println!();

    // 并行加载所有文件
    let start_time = Instant::now();
    let results = load_files_parallel(files);
    let total_load_time = start_time.elapsed();

    // 输出结果
    print_results(&results, total_load_time);
}

#[derive(Debug)]
struct LoadResult {
    file_path: PathBuf,
    success: bool,
    note_count: usize,
    bpm: f64,
    error: Option<String>,
    #[allow(dead_code)]
    load_time_ms: u64,
}

fn load_files_parallel(files: Vec<PathBuf>) -> Vec<LoadResult> {
    let (tx, rx) = mpsc::channel();
    let mut handles = Vec::new();

    // 为每个文件创建一个线程
    for (index, file) in files.into_iter().enumerate() {
        let tx = tx.clone();
        let handle = thread::spawn(move || {
            let result = load_single_file(&file, index);
            let _ = tx.send(result);
        });
        handles.push(handle);
    }

    // 收集所有结果
    let mut results = Vec::new();
    for _ in &handles {
        if let Ok(result) = rx.recv() {
            results.push(result);
        }
    }

    // 等待所有线程完成
    for handle in handles {
        let _ = handle.join();
    }

    // 按原始文件路径排序
    results.sort_by(|a, b| a.file_path.cmp(&b.file_path));
    results
}

fn load_single_file(path: &Path, _index: usize) -> LoadResult {
    let start_time = Instant::now();

    let reader = match MmapReader::open(path) {
        Ok(r) => r,
        Err(e) => {
            return LoadResult {
                file_path: path.to_path_buf(),
                success: false,
                note_count: 0,
                bpm: 0.0,
                error: Some(format!("无法打开文件: {}", e)),
                load_time_ms: 0,
            };
        }
    };

    let (_options, handle, reporter) = LoadOptions::new().with_progress();
    let loader = MmapMidiLoader::with_reporter(reporter);

    // 在后台线程接收进度（这里我们只消费但不显示）
    thread::spawn(move || {
        while let Ok(_event) = handle.recv() {
            // 进度事件被消耗，不做显示
        }
    });

    let mut note_count = 0usize;
    let mut tempo_sum: u64 = 0;
    let mut tempo_count: usize = 0;

    let result = loader.analyze_streaming(&reader, |track, _track_index| {
        use lumino_midiloader::mmap_model::FastEventKind;
        
        for event in track.iter_fast_events() {
            match event.kind {
                FastEventKind::NoteOn { .. } => {
                    note_count += 1;
                }
                FastEventKind::Tempo { tempo } => {
                    tempo_sum += tempo as u64;
                    tempo_count += 1;
                }
                _ => {}
            }
        }
        Ok(())
    });

    let load_time = start_time.elapsed();

    match result {
        Ok(_header) => {
            // 计算平均 BPM
            let bpm = if tempo_count > 0 {
                let avg_tempo = tempo_sum / tempo_count as u64;
                60_000_000.0 / avg_tempo as f64
            } else {
                // 默认 BPM 120
                120.0
            };

            LoadResult {
                file_path: path.to_path_buf(),
                success: true,
                note_count,
                bpm,
                error: None,
                load_time_ms: load_time.as_millis() as u64,
            }
        }
        Err(e) => LoadResult {
            file_path: path.to_path_buf(),
            success: false,
            note_count: 0,
            bpm: 0.0,
            error: Some(format!("解析错误: {}", e)),
            load_time_ms: load_time.as_millis() as u64,
        },
    }
}

fn print_results(results: &[LoadResult], total_time: std::time::Duration) {
    let successful: Vec<&LoadResult> = results.iter().filter(|r| r.success).collect();
    let failed: Vec<&LoadResult> = results.iter().filter(|r| !r.success).collect();

    // 输出成功加载的文件
    if !successful.is_empty() {
        println!("┌─────────────────────────────────────────────────────────────┐");
        println!("│ 成功加载的文件                                              │");
        println!("├─────────────────────────────────────────────────────────────┤");
        
        for result in &successful {
            let file_name = result
                .file_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("未知");
            println!(
                "│ {:<30} │ {:>6} 音符 │ {:>6.1} BPM │",
                file_name, result.note_count, result.bpm
            );
        }
        println!("└─────────────────────────────────────────────────────────────┘");
        println!();
    }

    // 输出失败的文件
    if !failed.is_empty() {
        println!("┌─────────────────────────────────────────────────────────────┐");
        println!("│ 加载失败的文件                                              │");
        println!("├─────────────────────────────────────────────────────────────┤");
        
        for result in &failed {
            let file_name = result
                .file_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("未知");
            let error = result.error.as_deref().unwrap_or("未知错误");
            println!("│ {:<30} │ {}", file_name, error);
        }
        println!("└─────────────────────────────────────────────────────────────┘");
        println!();
    }

    // 输出汇总统计
    let total_notes: usize = successful.iter().map(|r| r.note_count).sum();
    let avg_bpm = if !successful.is_empty() {
        successful.iter().map(|r| r.bpm).sum::<f64>() / successful.len() as f64
    } else {
        0.0
    };

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                        汇 总 统 计                           ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║ 成功加载: {:<50} ║", format!("{}/{} 文件", successful.len(), results.len()));
    println!("║ 总音符数: {:<50} ║", total_notes);
    println!("║ 平均 BPM: {:<50.1} ║", avg_bpm);
    println!("║ 总耗时:   {:<50} ║", format!("{:?}", total_time));
    println!("╚══════════════════════════════════════════════════════════════╝");
}
