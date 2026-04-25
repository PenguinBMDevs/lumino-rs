//! Lumino 项目集成测试
//!
//! 测试 MIDI/DMS/LMPJ 文件格式转换的准确性和性能

use std::path::PathBuf;

/// 获取测试文件路径
fn get_test_file_path(relative_path: &str) -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    PathBuf::from(manifest_dir)
        .join("test-file")
        .join(relative_path)
}

/// 测试 1: MIDI 转 DMS 数据相似度测试
///
/// 加载 dms-loader-test.mid，转换为 DMS 格式，
/// 与 dms-loader-test.dms 对比语义信息相似度
#[test]
fn test_midi_to_dms_similarity() {
    let midi_path = get_test_file_path("DMS-Loader/dms-loader-test.mid");
    let dms_reference_path = get_test_file_path("DMS-Loader/dms-loader-test.dms");

    assert!(midi_path.exists(), "MIDI 测试文件不存在: {:?}", midi_path);
    assert!(
        dms_reference_path.exists(),
        "DMS 参考文件不存在: {:?}",
        dms_reference_path
    );

    let info = lumino_core::MidiInfo::from_path(midi_path.clone()).expect("解析 MIDI 文件失败");

    println!("MIDI 文件信息:");
    println!("  音轨数量: {}", info.track_count);
    println!("  音符数量: {}", info.total_notes);
    println!("  PPQN: {}", info.division);

    let exported_dms_bytes =
        lumino_export::export_dms_from_midi_sync(&midi_path).expect("MIDI 转 DMS 失败");

    let reference_dms_bytes = std::fs::read(&dms_reference_path).expect("读取参考 DMS 文件失败");

    // 解析导出的 DMS 文件
    let exported_root =
        lumino_dms::read_dms_file(&exported_dms_bytes).expect("解析导出的 DMS 文件失败");

    // 解析参考 DMS 文件
    let ref_root = lumino_dms::read_dms_file(&reference_dms_bytes).expect("解析参考 DMS 文件失败");

    // 提取语义信息进行对比
    fn extract_dms_info(
        node: &dyn lumino_dms::DmsNode,
    ) -> (u64, usize, Option<u32>, Option<String>) {
        use lumino_dms::DmsNodeType;

        let mut note_count = 0u64;
        let mut track_count = 0usize;
        let mut ppqn: Option<u32> = None;
        let mut song_name: Option<String> = None;

        fn scan_node(
            node: &dyn lumino_dms::DmsNode,
            note_count: &mut u64,
            track_count: &mut usize,
            ppqn: &mut Option<u32>,
            song_name: &mut Option<String>,
        ) {
            let type_id = node.type_id();

            if type_id == DmsNodeType::TRACK {
                *track_count += 1;
            }
            if type_id == DmsNodeType::NOTE_EVENT {
                *note_count += 1;
            }
            if type_id == DmsNodeType::SONG_PPQN
                && let Some(int_node) = node.as_any().downcast_ref::<lumino_dms::DmsIntegerNode>()
            {
                let val = int_node.integer_data();
                *ppqn = val.to_string().parse().ok();
            }
            if type_id == DmsNodeType::SONG_NAME
                && let Some(str_node) = node
                    .as_any()
                    .downcast_ref::<lumino_dms::DmsAnsiStringNode>()
            {
                *song_name = str_node.string_data().ok();
            }

            for child in node.children() {
                scan_node(child.as_ref(), note_count, track_count, ppqn, song_name);
            }
        }

        scan_node(
            node,
            &mut note_count,
            &mut track_count,
            &mut ppqn,
            &mut song_name,
        );
        (note_count, track_count, ppqn, song_name)
    }

    let (exported_notes, exported_tracks, exported_ppqn, exported_name) =
        extract_dms_info(&exported_root);
    let (ref_notes, ref_tracks, ref_ppqn, ref_name) = extract_dms_info(&ref_root);

    println!("\n导出 DMS 文件信息:");
    println!("  音轨数量: {}", exported_tracks);
    println!("  音符数量: {}", exported_notes);
    println!("  PPQN: {:?}", exported_ppqn);
    println!("  歌曲名称: {:?}", exported_name);
    println!("  文件大小: {} bytes", exported_dms_bytes.len());

    println!("\n参考 DMS 文件信息:");
    println!("  音轨数量: {}", ref_tracks);
    println!("  音符数量: {}", ref_notes);
    println!("  PPQN: {:?}", ref_ppqn);
    println!("  歌曲名称: {:?}", ref_name);
    println!("  文件大小: {} bytes", reference_dms_bytes.len());

    // 计算语义相似度
    let mut similarity_score = 0.0;
    let mut total_checks = 0.0;

    // 检查音符数量匹配
    if ref_notes > 0 {
        let note_similarity = if exported_notes == ref_notes {
            100.0
        } else {
            let diff = (exported_notes as f64 - ref_notes as f64).abs();
            (100.0 - (diff / ref_notes as f64 * 100.0)).max(0.0)
        };
        similarity_score += note_similarity;
        total_checks += 1.0;
        println!("\n音符数量相似度: {:.2}%", note_similarity);
    }

    // 检查音轨数量匹配
    if ref_tracks > 0 {
        let track_similarity = if exported_tracks == ref_tracks {
            100.0
        } else {
            let diff = (exported_tracks as f64 - ref_tracks as f64).abs();
            (100.0 - (diff / ref_tracks as f64 * 100.0)).max(0.0)
        };
        similarity_score += track_similarity;
        total_checks += 1.0;
        println!("音轨数量相似度: {:.2}%", track_similarity);
    }

    // 检查 PPQN 匹配
    if let Some(ref_ppqn_val) = ref_ppqn {
        let ppqn_similarity = if exported_ppqn == Some(ref_ppqn_val) {
            100.0
        } else {
            0.0
        };
        similarity_score += ppqn_similarity;
        total_checks += 1.0;
        println!("PPQN 相似度: {:.2}%", ppqn_similarity);
    }

    // 计算总相似度
    let total_similarity = if total_checks > 0.0 {
        similarity_score / total_checks
    } else {
        0.0
    };

    println!("\n总体语义相似度: {:.2}%", total_similarity);

    assert!(
        total_similarity > 95.0,
        "DMS 语义相似度 {:.2}% 不满足要求 (> 95%)",
        total_similarity
    );
}

/// 测试 2: DMS 文件元数据验证测试
///
/// 使用 dms-loader-test.dms 验证音符数量和音轨数量
#[test]
fn test_dms_metadata_validation() {
    // 使用较小的测试文件
    let dms_path = get_test_file_path("DMS-Loader/dms-loader-test.dms");

    assert!(dms_path.exists(), "DMS 测试文件不存在: {:?}", dms_path);

    // 打印文件大小
    let file_metadata = std::fs::metadata(&dms_path).expect("获取文件元数据失败");
    let file_size_kb = file_metadata.len() as f64 / 1024.0;
    println!("DMS 文件大小: {:.2} KB", file_size_kb);

    // 使用完整解析来验证
    let dms_bytes = std::fs::read(&dms_path).expect("读取 DMS 文件失败");

    let root = lumino_dms::read_dms_file(&dms_bytes).expect("解析 DMS 文件失败");

    // 提取语义信息
    fn extract_dms_info(
        node: &dyn lumino_dms::DmsNode,
    ) -> (u64, usize, Option<u32>, Option<String>) {
        use lumino_dms::DmsNodeType;

        let mut note_count = 0u64;
        let mut track_count = 0usize;
        let mut ppqn: Option<u32> = None;
        let mut song_name: Option<String> = None;

        fn scan_node(
            node: &dyn lumino_dms::DmsNode,
            note_count: &mut u64,
            track_count: &mut usize,
            ppqn: &mut Option<u32>,
            song_name: &mut Option<String>,
        ) {
            let type_id = node.type_id();

            if type_id == DmsNodeType::TRACK {
                *track_count += 1;
            }
            if type_id == DmsNodeType::NOTE_EVENT {
                *note_count += 1;
            }
            if type_id == DmsNodeType::SONG_PPQN
                && let Some(int_node) = node.as_any().downcast_ref::<lumino_dms::DmsIntegerNode>()
            {
                let val = int_node.integer_data();
                *ppqn = val.to_string().parse().ok();
            }
            if type_id == DmsNodeType::SONG_NAME
                && let Some(str_node) = node
                    .as_any()
                    .downcast_ref::<lumino_dms::DmsAnsiStringNode>()
            {
                *song_name = str_node.string_data().ok();
            }

            for child in node.children() {
                scan_node(child.as_ref(), note_count, track_count, ppqn, song_name);
            }
        }

        scan_node(
            node,
            &mut note_count,
            &mut track_count,
            &mut ppqn,
            &mut song_name,
        );
        (note_count, track_count, ppqn, song_name)
    }

    let (note_count, track_count, ppqn, song_name) = extract_dms_info(&root);

    println!("DMS 文件: {:?}", dms_path);
    println!("音符数量: {}", note_count);
    println!("音轨数量: {}", track_count);
    println!("歌曲名称: {:?}", song_name);
    println!("PPQN: {:?}", ppqn);

    // 验证基本元数据存在
    assert!(note_count > 0, "音符数量 {} 不满足要求 (> 0)", note_count);

    assert!(track_count > 0, "音轨数量 {} 不满足要求 (> 0)", track_count);
}

/// 测试 3: 大文件内存占用测试
///
/// 加载 Rekt Apple!!.mid，验证内存占用不超过 30MB
/// 使用与主程序相同的流式扫描逻辑
#[test]
#[cfg(target_os = "windows")]
fn test_midi_memory_usage() {
    let midi_path = get_test_file_path("MIDI-Loader/Rekt Apple!!.mid");
    assert!(midi_path.exists(), "MIDI 测试文件不存在: {:?}", midi_path);

    let initial_memory = get_process_memory_kb();

    let info = lumino_core::MidiInfo::from_path(midi_path.clone()).expect("解析 MIDI 文件失败");

    let after_memory = get_process_memory_kb();
    let memory_delta_mb = (after_memory.saturating_sub(initial_memory)) as f64 / 1024.0;

    println!("MIDI 文件: {:?}", midi_path);
    println!("音轨数量: {}", info.track_count);
    println!("音符数量: {}", info.total_notes);
    println!("初始内存: {} KB", initial_memory);
    println!("加载后内存: {} KB", after_memory);
    println!("内存增量: {:.2} MB", memory_delta_mb);

    assert!(
        memory_delta_mb <= 30.0,
        "内存增量 {:.2} MB 超过限制 (30 MB)",
        memory_delta_mb
    );

    drop(info);
}

/// 获取当前进程内存占用（KB）
#[cfg(target_os = "windows")]
fn get_process_memory_kb() -> u64 {
    use std::mem::MaybeUninit;
    use winapi::um::processthreadsapi::GetCurrentProcess;
    use winapi::um::psapi::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};

    unsafe {
        let mut counters: PROCESS_MEMORY_COUNTERS = MaybeUninit::zeroed().assume_init();
        counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;

        let result = GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb);

        if result != 0 {
            (counters.WorkingSetSize / 1024) as u64
        } else {
            0
        }
    }
}

/// 测试 4: MIDI-LMPJ 往返转换测试
///
/// 加载 Internet Yamero.mid，转换为 LMPJ，再转回 MIDI，
/// 验证语义信息完全匹配
#[test]
fn test_midi_lmpj_roundtrip() {
    let midi_path = get_test_file_path("LMPJ-Exporter/Internet Yamero.mid");

    assert!(midi_path.exists(), "MIDI 测试文件不存在: {:?}", midi_path);

    let original_midi_bytes = std::fs::read(&midi_path).expect("读取原始 MIDI 文件失败");

    let info = lumino_core::MidiInfo::from_path(midi_path.clone()).expect("解析 MIDI 文件失败");

    let parsed_midi = lumino_core::midi::ParsedMidi {
        info: info.clone(),
        midi_data: Some(original_midi_bytes.clone()),
        memory_manager: None,
        cache: None,
    };

    let temp_dir = std::env::temp_dir();
    let lmpj_path = temp_dir.join("lumino_test_roundtrip.lmpj");

    lumino_export::save_sync(&parsed_midi, &lmpj_path).expect("保存 LMPJ 文件失败");

    let lmpj_bytes = std::fs::read(&lmpj_path).expect("读取 LMPJ 文件失败");

    let _parsed_from_lmpj: lumino_core::midi::ParsedMidi =
        lumino_export::format::decode_lmpj(&lmpj_bytes).expect("解码 LMPJ 文件失败");

    let roundtrip_midi_path = temp_dir.join("lumino_test_roundtrip_1.mid");

    let exported_midi_bytes = lumino_export::export_midi_from_parsed_midi_sync(&lmpj_path)
        .expect("从 LMPJ 导出 MIDI 失败");

    std::fs::write(&roundtrip_midi_path, &exported_midi_bytes).expect("写入往返 MIDI 文件失败");

    // 对比语义信息
    let original_info =
        lumino_core::MidiInfo::from_path(midi_path.clone()).expect("解析原始 MIDI 文件失败");
    let roundtrip_info = lumino_core::MidiInfo::from_path(roundtrip_midi_path.clone())
        .expect("解析往返 MIDI 文件失败");

    println!("原始 MIDI 大小: {} bytes", original_midi_bytes.len());
    println!("LMPJ 大小: {} bytes", lmpj_bytes.len());
    println!("往返 MIDI 大小: {} bytes", exported_midi_bytes.len());
    println!("\n原始 MIDI 信息:");
    println!("  音轨数量: {}", original_info.track_count);
    println!("  音符数量: {}", original_info.total_notes);
    println!("  PPQN: {}", original_info.division);
    println!("\n往返 MIDI 信息:");
    println!("  音轨数量: {}", roundtrip_info.track_count);
    println!("  音符数量: {}", roundtrip_info.total_notes);
    println!("  PPQN: {}", roundtrip_info.division);

    // 验证语义信息完全匹配
    assert!(
        original_info.track_count == roundtrip_info.track_count,
        "音轨数量不匹配: {} vs {}",
        original_info.track_count,
        roundtrip_info.track_count
    );

    assert!(
        original_info.total_notes == roundtrip_info.total_notes,
        "音符数量不匹配: {} vs {}",
        original_info.total_notes,
        roundtrip_info.total_notes
    );

    assert!(
        original_info.division == roundtrip_info.division,
        "PPQN 不匹配: {} vs {}",
        original_info.division,
        roundtrip_info.division
    );

    let _ = std::fs::remove_file(&lmpj_path);
    let _ = std::fs::remove_file(&roundtrip_midi_path);
}
