//! MIDI 输入手动测试脚本
//!
//! 用于连接真实 MIDI 设备验证录制功能。
//!
//! 运行方式：
//!   cargo test --test midi_input_manual -- --ignored --nocapture
//!
//! 或直接作为二进制运行（需要添加 [[bin]] 到 Cargo.toml）：
//!   cargo run --bin midi_input_manual
//!
//! 测试内容：
//! 1. 枚举系统 MIDI 输入设备
//! 2. 打开第一个可用设备
//! 3. 实时显示接收到的 MIDI 消息
//! 4. 模拟录制流程并验证结果

use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// MIDI 状态字节定义
const STATUS_NOTE_ON: u8 = 0x90;
const STATUS_NOTE_OFF: u8 = 0x80;
const STATUS_CONTROL_CHANGE: u8 = 0xB0;
const STATUS_PROGRAM_CHANGE: u8 = 0xC0;
const STATUS_PITCH_BEND: u8 = 0xE0;

/// 打印彩色日志的辅助宏
macro_rules! info {
    ($($arg:tt)*) => {
        println!("\x1b[36m[INFO]\x1b[0m {}", format!($($arg)*));
    };
}

macro_rules! success {
    ($($arg:tt)*) => {
        println!("\x1b[32m[OK]\x1b[0m   {}", format!($($arg)*));
    };
}

macro_rules! warn {
    ($($arg:tt)*) => {
        println!("\x1b[33m[WARN]\x1b[0m {}", format!($($arg)*));
    };
}

macro_rules! error {
    ($($arg:tt)*) => {
        println!("\x1b[31m[ERR]\x1b[0m  {}", format!($($arg)*));
    };
}

/// 解析并打印 MIDI 消息
fn print_midi_message(data: &[u8]) {
    if data.is_empty() {
        return;
    }

    let status = data[0];
    let msg_type = status & 0xF0;
    let channel = status & 0x0F;

    match msg_type {
        STATUS_NOTE_ON if data.len() >= 3 => {
            let key = data[1];
            let vel = data[2];
            if vel == 0 {
                println!(
                    "  \x1b[35mNoteOff\x1b[0m  ch={:2} key={:3} ({:8}) vel={:3}  [零力度 NoteOn 转换]",
                    channel,
                    key,
                    note_name(key),
                    vel
                );
            } else {
                println!(
                    "  \x1b[32mNoteOn\x1b[0m   ch={:2} key={:3} ({:8}) vel={:3}",
                    channel,
                    key,
                    note_name(key),
                    vel
                );
            }
        }
        STATUS_NOTE_OFF if data.len() >= 3 => {
            let key = data[1];
            let vel = data[2];
            println!(
                "  \x1b[35mNoteOff\x1b[0m  ch={:2} key={:3} ({:8}) vel={:3}",
                channel,
                key,
                note_name(key),
                vel
            );
        }
        STATUS_CONTROL_CHANGE if data.len() >= 3 => {
            println!(
                "  \x1b[33mCC\x1b[0m       ch={:2} ctrl={:3} val={:3}",
                channel, data[1], data[2]
            );
        }
        STATUS_PROGRAM_CHANGE if data.len() >= 2 => {
            println!(
                "  \x1b[34mPC\x1b[0m       ch={:2} prog={:3}",
                channel, data[1]
            );
        }
        STATUS_PITCH_BEND if data.len() >= 3 => {
            let value = ((data[2] as i16) << 7) | (data[1] as i16);
            println!(
                "  \x1b[34mPitchBend\x1b[0m ch={:2} val={:5}",
                channel, value
            );
        }
        _ => {
            print!("  Raw: [");
            for (i, b) in data.iter().enumerate() {
                if i > 0 {
                    print!(", ");
                }
                print!("{:02X}", b);
            }
            println!("]");
        }
    }
}

/// 将 MIDI 键号转换为音符名称
fn note_name(key: u8) -> String {
    let names = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let octave = (key / 12) as i8 - 1;
    let note = names[(key % 12) as usize];
    format!("{}{}", note, octave)
}

/// 主测试流程
fn run_manual_test() {
    println!("\n{}", "═".repeat(60));
    println!("  Lumino MIDI 输入手动测试");
    println!("{}", "═".repeat(60));

    // 步骤 1：创建 MIDI API
    info!("步骤 1/5: 初始化 MIDI API...");
    let api = match lumino_midi_io::new_api(&lumino_midi_io::ApiKind::System) {
        Ok(api) => {
            success!("System MIDI API 初始化成功");
            api
        }
        Err(e) => {
            error!("System MIDI API 初始化失败: {:?}", e);
            std::process::exit(1);
        }
    };

    // 步骤 2：枚举输入设备
    info!("步骤 2/5: 枚举 MIDI 输入设备...");
    let inputs = match api.inputs() {
        Ok(devices) => {
            if devices.is_empty() {
                error!("未找到任何 MIDI 输入设备");
                error!("请连接 MIDI 键盘或创建虚拟 MIDI 端口（如 loopMIDI）后重试");
                std::process::exit(1);
            }
            success!("发现 {} 个输入设备:", devices.len());
            for (i, dev) in devices.iter().enumerate() {
                println!("    [{}] {} (ID: {})", i, dev.name, dev.id);
            }
            devices
        }
        Err(e) => {
            error!("获取输入设备列表失败: {:?}", e);
            std::process::exit(1);
        }
    };

    // 步骤 3：选择设备
    info!("步骤 3/5: 选择输入设备...");
    let device = if inputs.len() == 1 {
        info!("只有一个设备，自动选择: {}", inputs[0].name);
        &inputs[0]
    } else {
        print!("请输入设备编号 [0-{}]: ", inputs.len() - 1);
        io::stdout().flush().expect("刷新标准输出失败");
        let mut input = String::new();
        io::stdin().read_line(&mut input).expect("读取用户输入失败");
        let idx: usize = input.trim().parse().unwrap_or(0);
        &inputs[idx.min(inputs.len() - 1)]
    };
    success!("已选择设备: {} (ID: {})", device.name, device.id);

    // 步骤 4：打开输入连接
    info!("步骤 4/5: 打开 MIDI 输入连接...");
    let buffer: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
    let buffer_clone = Arc::clone(&buffer);

    let conn = match api.open_input(
        device.id,
        Box::new(move |_timestamp: u64, data: &[u8]| {
            if let Ok(mut buf) = buffer_clone.lock() {
                buf.push(data.to_vec());
            }
        }),
    ) {
        Ok(conn) => {
            success!("输入连接已打开");
            conn
        }
        Err(e) => {
            error!("打开输入连接失败: {:?}", e);
            std::process::exit(1);
        }
    };

    // 步骤 5：监听并显示 MIDI 消息
    println!("\n{}", "─".repeat(60));
    println!("  现在开始监听 MIDI 输入...");
    println!("  请在 MIDI 设备上演奏音符");
    println!("  按 Enter 键停止监听\n");
    println!("{}", "─".repeat(60));

    let start_time = Instant::now();
    let mut total_messages = 0usize;
    let mut note_on_count = 0usize;
    let mut note_off_count = 0usize;

    // 使用非阻塞方式读取用户输入
    let running = Arc::new(Mutex::new(true));
    let running_clone = Arc::clone(&running);

    std::thread::spawn(move || {
        let mut input = String::new();
        io::stdin().read_line(&mut input).ok();
        *running_clone.lock().expect("锁定运行状态标志失败") = false;
    });

    loop {
        // 检查是否应该退出
        if !*running.lock().expect("锁定运行状态标志失败") {
            break;
        }

        // 处理缓冲区中的 MIDI 消息
        let messages: Vec<Vec<u8>> = {
            if let Ok(mut buf) = buffer.lock() {
                let msgs = buf.clone();
                buf.clear();
                msgs
            } else {
                vec![]
            }
        };

        for data in messages {
            total_messages += 1;
            print_midi_message(&data);

            // 统计
            if data.len() >= 3 {
                let status = data[0];
                let msg_type = status & 0xF0;
                let vel = data[2];

                match msg_type {
                    STATUS_NOTE_ON if vel > 0 => note_on_count += 1,
                    STATUS_NOTE_OFF | STATUS_NOTE_ON => note_off_count += 1,
                    _ => {}
                }
            }
        }

        std::thread::sleep(Duration::from_millis(10));
    }

    // 关闭连接
    drop(conn);
    success!("输入连接已关闭");

    // 最终报告
    let duration = start_time.elapsed();
    println!("\n{}", "═".repeat(60));
    println!("  测试报告");
    println!("{}", "═".repeat(60));
    success!("运行时间: {:.1} 秒", duration.as_secs_f64());
    success!("总消息数: {}", total_messages);
    success!("NoteOn 数: {}", note_on_count);
    success!("NoteOff 数: {}", note_off_count);

    if total_messages == 0 {
        warn!("未接收到任何 MIDI 消息");
        warn!("可能原因:");
        warn!("  1. MIDI 设备未正确连接");
        warn!("  2. MIDI 设备未发送数据");
        warn!("  3. 使用了错误的 MIDI 端口");
    } else {
        success!("MIDI 输入测试通过 ✓");
    }

    // 验证基本断言
    println!("\n验证:");
    assert!(
        total_messages > 0,
        "必须接收到至少一条 MIDI 消息才算测试通过"
    );
    println!("  ✓ 接收到 MIDI 消息");

    if note_on_count > 0 && note_off_count > 0 {
        println!("  ✓ 检测到 NoteOn 和 NoteOff 事件");
        success!("录制功能验证通过！MIDI 输入链路工作正常");
    } else if note_on_count > 0 {
        warn!("只检测到 NoteOn，未检测到 NoteOff");
        warn!("建议检查 MIDI 设备是否正确发送 NoteOff");
    }
}

/// 入口函数
#[test]
#[ignore = "需要真实 MIDI 硬件，手动运行: cargo test --test midi_input_manual -- --ignored --nocapture"]
fn test_midi_input_with_real_hardware() {
    run_manual_test();
}

/// 二进制入口（如果作为 bin 运行）
fn main() {
    run_manual_test();
}
