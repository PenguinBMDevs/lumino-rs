//! 探针子进程 stderr 的消费。
//!
//! 必须**并发**读取：父进程若只等子进程退出再读，stderr 管道写满（约 64KB）会把
//! 子进程阻塞在写日志上，形成「明明检测完了却永远不退出」的假挂起。
//! 同时只保留尾部 N 字节——异常子进程可能刷屏。

use std::io::Read;

/// stderr 诊断尾部上限（防止异常子进程刷屏占内存）
const TAIL_LIMIT: usize = 4096;

/// 后台读取 stderr 尾部；子进程退出（管道关闭）后 join 得到内容。
pub(super) fn spawn_reader(
    mut stderr: std::process::ChildStderr,
) -> Option<std::thread::JoinHandle<String>> {
    std::thread::Builder::new()
        .name("lumino-probe-stderr".into())
        .spawn(move || {
            let mut collected = Vec::new();
            let mut chunk = [0u8; 1024];
            loop {
                match stderr.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => {
                        if collected.len() < TAIL_LIMIT {
                            let take = read.min(TAIL_LIMIT - collected.len());
                            collected.extend_from_slice(&chunk[..take]);
                        }
                    }
                }
            }
            String::from_utf8_lossy(&collected).trim().to_string()
        })
        .ok()
}
