//! DMS 扫描调试测试

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

const TEST_TIMEOUT_SECS: u64 = 10; // 10秒超时用于调试

/// 调试 DMS 扫描 - 检查文件头
#[test]
#[ignore = "需要外部 DMS 文件"]
fn test_dms_debug_header() {
    let test_path = std::env::var("LUMINO_TEST_DMS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(r"E:\工程文件\MIDI创作\待编辑\warma审判曲\拼合成果.dms"));

    if !test_path.exists() {
        println!("测试文件不存在，跳过测试: {:?}", test_path);
        return;
    }

    println!("调试 DMS 文件: {:?}", test_path);

    // 读取文件头
    let bytes = std::fs::read(&test_path).expect("读取文件失败");

    // 检查魔数
    let magic = &bytes[0..lumino_dms::MAGIC_LENGTH];
    println!("魔数: {:?}", std::str::from_utf8(magic));

    // 检查解压长度
    let decompressed_len = u32::from_le_bytes([
        bytes[lumino_dms::MAGIC_LENGTH],
        bytes[lumino_dms::MAGIC_LENGTH + 1],
        bytes[lumino_dms::MAGIC_LENGTH + 2],
        bytes[lumino_dms::MAGIC_LENGTH + 3],
    ]);
    println!("解压长度: {} 字节", decompressed_len);
    println!("文件总大小: {} 字节", bytes.len());

    // 尝试只读取前1KB数据看看
    println!("\n前100字节 (hex):");
    for (i, byte) in bytes.iter().take(100).enumerate() {
        if i % 16 == 0 {
            print!("\n{:04x}: ", i);
        }
        print!("{:02x} ", byte);
    }
    println!();
}

/// 调试 DMS 扫描 - 使用更短的超时
#[test]
#[ignore = "需要外部 DMS 文件"]
fn test_dms_debug_scan() {
    let test_path = std::env::var("LUMINO_TEST_DMS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(r"E:\工程文件\MIDI创作\待编辑\warma审判曲\拼合成果.dms"));

    if !test_path.exists() {
        println!("测试文件不存在，跳过测试: {:?}", test_path);
        return;
    }

    println!("调试 DMS 扫描: {:?}", test_path);
    println!("超时时间: {} 秒", TEST_TIMEOUT_SECS);

    let (tx, rx) = mpsc::channel();
    let path = test_path.clone();

    let handle = thread::spawn(move || {
        let file = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(e) => {
                let _ = tx.send(Err(format!("打开文件失败: {}", e)));
                return;
            }
        };

        let file_size = match file.metadata() {
            Ok(m) => m.len(),
            Err(e) => {
                let _ = tx.send(Err(format!("获取文件元数据失败: {}", e)));
                return;
            }
        };
        println!("文件大小: {} 字节", file_size);

        let mut reader = std::io::BufReader::new(file);

        println!("开始扫描...");
        match lumino_dms::scan_dms_streaming(&mut reader) {
            Ok(result) => {
                println!("扫描完成!");
                let _ = tx.send(Ok(result));
            }
            Err(e) => {
                println!("扫描失败: {}", e);
                let _ = tx.send(Err(format!("扫描失败: {}", e)));
            }
        }
    });

    match rx.recv_timeout(Duration::from_secs(TEST_TIMEOUT_SECS)) {
        Ok(Ok(result)) => {
            handle.join().expect("线程 join 失败");
            println!("扫描结果:");
            println!("  - 轨道数: {}", result.track_count);
            println!("  - 音符数: {}", result.total_notes);
        }
        Ok(Err(e)) => {
            handle.join().expect("线程 join 失败");
            panic!("{}", e);
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            println!("\n❌ 测试超时！{} 秒内未完成", TEST_TIMEOUT_SECS);
            panic!("扫描超时");
        }
        Err(e) => {
            panic!("接收结果失败: {:?}", e);
        }
    }
}
