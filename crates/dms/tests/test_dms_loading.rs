//! DMS 加载流程自动化测试
//!
//! 测试文件: E:\工程文件\MIDI创作\待编辑\warma审判曲\拼合成果.dms

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

const TEST_TIMEOUT_SECS: u64 = 80; // 80秒超时

/// 测试 DMS 文件加载的完整流程（带超时）
#[test]
fn test_dms_full_loading_pipeline() {
    // 使用用户提供的测试文件路径
    let test_path = PathBuf::from(r"E:\工程文件\MIDI创作\待编辑\warma审判曲\拼合成果.dms");

    // 检查文件是否存在
    if !test_path.exists() {
        println!("测试文件不存在，跳过测试: {:?}", test_path);
        return;
    }

    println!("开始测试 DMS 加载流程: {:?}", test_path);
    println!("测试超时时间: {} 秒", TEST_TIMEOUT_SECS);

    // 使用通道和线程实现超时控制
    let (tx, rx) = mpsc::channel();
    let path = test_path.clone();

    let handle = thread::spawn(move || {
        let result = run_dms_test(&path);
        let _ = tx.send(result);
    });

    // 等待测试结果或超时
    match rx.recv_timeout(Duration::from_secs(TEST_TIMEOUT_SECS)) {
        Ok(result) => {
            handle.join().expect("线程 join 失败");
            result.expect("测试失败");
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            println!("\n❌ 测试超时！{} 秒内未完成", TEST_TIMEOUT_SECS);
            println!("可能原因: scan_dms_streaming 死循环");
            panic!("测试超时");
        }
        Err(e) => {
            panic!("接收结果失败: {:?}", e);
        }
    }

    println!("\n✓ 所有测试通过!");
}

fn run_dms_test(test_path: &PathBuf) -> Result<(), String> {
    // 步骤1: 测试流式扫描
    println!("\n[测试] 步骤1: 流式扫描 DMS 文件");
    let scan_result = {
        let file = std::fs::File::open(test_path).map_err(|e| e.to_string())?;
        let mut reader = std::io::BufReader::new(file);
        lumino_dms::scan_dms_streaming(&mut reader).map_err(|e| e.to_string())?
    };

    println!("  ✓ 扫描成功");
    println!("    - 轨道数: {}", scan_result.track_count);
    println!("    - 音符数: {}", scan_result.total_notes);
    println!("    - 歌曲名: {:?}", scan_result.song_name);
    println!("    - PPQN: {:?}", scan_result.ppqn);

    // 步骤2: 测试完整数据加载
    println!("\n[测试] 步骤2: 加载完整 DMS 数据");
    let lightweight_data = {
        let bytes = std::fs::read(test_path).map_err(|e| e.to_string())?;
        println!("  文件大小: {} 字节", bytes.len());
        lumino_dms::read_dms_lightweight(&bytes).map_err(|e| e.to_string())?
    };

    println!("  ✓ 数据加载成功");
    println!("    - 解压后大小: {} 字节", lightweight_data.len());

    // 步骤3: 测试完整解析
    println!("\n[测试] 步骤3: 解析完整 DMS 节点树");
    match lightweight_data.parse_full() {
        Ok(root) => {
            println!("  ✓ 解析成功");
            println!("    - 根节点子节点数: {}", root.children.len());

            // 统计轨道数量
            let track_count = root
                .children
                .iter()
                .filter(|child| child.type_id() == lumino_dms::DmsNodeType::TRACK)
                .count();
            println!("    - 轨道节点数: {}", track_count);
        }
        Err(e) => {
            return Err(e.to_string());
        }
    }

    Ok(())
}

/// 测试 DMS 文件格式验证（带超时）
#[test]
fn test_dms_file_format_validation() {
    let test_path = PathBuf::from(r"E:\工程文件\MIDI创作\待编辑\warma审判曲\拼合成果.dms");

    if !test_path.exists() {
        println!("测试文件不存在，跳过测试: {:?}", test_path);
        return;
    }

    println!("验证 DMS 文件格式: {:?}", test_path);

    // 读取文件头
    let bytes = std::fs::read(&test_path).expect("读取文件失败");

    // 检查魔数
    let magic = &bytes[0..lumino_dms::MAGIC_LENGTH];
    assert_eq!(magic, lumino_dms::DMS_MAGIC, "DMS 魔数不匹配");
    println!(
        "  ✓ 魔数验证通过: {:?}",
        std::str::from_utf8(magic).unwrap_or("<无效UTF8>")
    );

    // 检查解压长度
    let decompressed_len = u32::from_le_bytes([
        bytes[lumino_dms::MAGIC_LENGTH],
        bytes[lumino_dms::MAGIC_LENGTH + 1],
        bytes[lumino_dms::MAGIC_LENGTH + 2],
        bytes[lumino_dms::MAGIC_LENGTH + 3],
    ]);
    println!("  ✓ 解压长度: {} 字节", decompressed_len);

    // 检查文件总大小
    println!("  ✓ 文件总大小: {} 字节", bytes.len());
}

/// 测试 DMS 扫描功能（带超时）
#[test]
fn test_dms_scan_only() {
    let test_path = PathBuf::from(r"E:\工程文件\MIDI创作\待编辑\warma审判曲\拼合成果.dms");

    if !test_path.exists() {
        println!("测试文件不存在，跳过测试: {:?}", test_path);
        return;
    }

    println!("测试 DMS 扫描: {:?}", test_path);
    println!("测试超时时间: {} 秒", TEST_TIMEOUT_SECS);

    // 使用通道和线程实现超时控制
    let (tx, rx) = mpsc::channel();
    let path = test_path.clone();

    let handle = thread::spawn(move || {
        let file = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(e) => {
                let _ = tx.send(Err(e.to_string()));
                return;
            }
        };
        let mut reader = std::io::BufReader::new(file);

        match lumino_dms::scan_dms_streaming(&mut reader) {
            Ok(result) => {
                let _ = tx.send(Ok(result));
            }
            Err(e) => {
                let _ = tx.send(Err(e.to_string()));
            }
        }
    });

    // 等待测试结果或超时
    match rx.recv_timeout(Duration::from_secs(TEST_TIMEOUT_SECS)) {
        Ok(Ok(result)) => {
            handle.join().expect("线程 join 失败");

            println!("扫描结果:");
            println!("  - 轨道数: {}", result.track_count);
            println!("  - 音符数: {}", result.total_notes);
            println!("  - 歌曲名: {:?}", result.song_name);
            println!("  - 版权: {:?}", result.copyright);
            println!("  - 注释: {:?}", result.comment);
            println!("  - PPQN: {:?}", result.ppqn);
            println!("  - 工作时间: {:?} 秒", result.working_time_sec);

            // 基本验证
            assert!(result.track_count > 0, "轨道数应该大于0");
        }
        Ok(Err(e)) => {
            handle.join().expect("线程 join 失败");
            panic!("扫描失败: {}", e);
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            println!("\n❌ 测试超时！{} 秒内未完成", TEST_TIMEOUT_SECS);
            println!("可能原因: scan_dms_streaming 死循环");
            panic!("测试超时");
        }
        Err(e) => {
            panic!("接收结果失败: {:?}", e);
        }
    }
}
