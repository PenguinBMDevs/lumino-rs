use std::sync::mpsc::Receiver;
use std::time::Duration;

use super::super::commands::{ControlCommand, RenderCommand};
use super::super::params::RenderParams;

/// 命令接收阻塞超时：防止渲染线程在 `recv()` 上无限阻塞，
/// 导致后台 贴图瀑布流 channel 不被消费（死锁）。
/// 超时后醒来检查 贴图瀑布流 channel + 渲染帧，然后重新进入等待。
const COMMAND_RECV_TIMEOUT: Duration = Duration::from_millis(16);

/// 处理渲染命令
///
/// 先 drain 所有积压命令，然后**仅当本轮没有收到新渲染命令时**才限时阻塞等待。
/// 返回 true 表示**本轮实际收到了新的渲染命令**（而非"已有参数"）。
///
/// **关键修复（GPU 满载根因）**：旧实现返回 `latest_params.is_some()`，
/// 是黏性的——首帧收到 `Render` 命令后 `latest_params` 永远为 `Some`，
/// 导致渲染线程 `while running` 每轮都执行离屏渲染，且因 `is_some()` 跳过
/// `recv_timeout` 阻塞，循环内无任何阻塞点 → 忙循环空转 → GPU 100% 满载。
/// 现改为"仅在有新 `Render` 命令时才渲染"，空闲时阻塞在 `recv_timeout` 休眠，
/// 既消除忙循环空转，又保留每 16ms 周期检查导出/贴图瀑布流后台通道（避免死锁）。
///
/// 贴图瀑布流控制命令（Generate/Dispose）需要 device/queue 上下文，无法在此处理，
/// 因此收集到 `deferred` 供主循环在拥有 GPU 资源时处理。
///
/// 使用 `recv_timeout` 而非 `recv`：防止渲染线程在无命令时无限阻塞，
/// 确保定期返回主循环以消费后台 贴图瀑布流 channel（避免死锁）。
pub fn process_commands(
    command_receiver: &Receiver<RenderCommand>,
    latest_params: &mut Option<RenderParams>,
    latest_frame_id: &mut u64,
    should_shutdown: &mut bool,
    deferred: &mut Vec<ControlCommand>,
) -> bool {
    // 先 drain 所有可用命令；记录本帧是否收到"新渲染命令"
    let mut new_render = false;
    while let Ok(cmd) = command_receiver.try_recv() {
        if classify_command(
            cmd,
            latest_params,
            latest_frame_id,
            should_shutdown,
            deferred,
        ) {
            new_render = true;
        }
    }

    // 如果已经收到 shutdown，直接返回
    if *should_shutdown {
        return false;
    }

    // 仅当本帧没有新渲染命令时才限时阻塞等待下一个命令。
    // 有则立即返回（new_render=true），不浪费一帧；无则休眠等待 UI 线程的下一帧请求。
    if !new_render {
        match command_receiver.recv_timeout(COMMAND_RECV_TIMEOUT) {
            Ok(cmd) => {
                if classify_command(
                    cmd,
                    latest_params,
                    latest_frame_id,
                    should_shutdown,
                    deferred,
                ) {
                    new_render = true;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                tracing::info!("Render thread: command channel closed");
                *should_shutdown = true;
            }
        }
    }

    new_render
}

/// 将单条命令分类处理
///
/// 返回 `true` 表示这是一条 `Render` 渲染命令（即"需要渲染新的一帧"）。
fn classify_command(
    cmd: RenderCommand,
    latest_params: &mut Option<RenderParams>,
    latest_frame_id: &mut u64,
    should_shutdown: &mut bool,
    deferred: &mut Vec<ControlCommand>,
) -> bool {
    match cmd {
        RenderCommand::Render { params, frame_id } => {
            *latest_params = Some(*params);
            // 记录最后一条 Render 命令的 frame_id，供主循环渲染完成后通知 UI
            *latest_frame_id = frame_id;
            true
        }
        RenderCommand::Control(ControlCommand::Shutdown) => {
            *should_shutdown = true;
            false
        }
        RenderCommand::Control(ControlCommand::Resize { width, height }) => {
            tracing::debug!("Render thread: resize to {}x{}", width, height);
            false
        }
        // 贴图瀑布流/视频导出控制命令需要 GPU 资源上下文，延迟到主循环处理
        RenderCommand::Control(
            cmd @ (ControlCommand::Waterfall(..)
            | ControlCommand::StartVideoExport { .. }
            | ControlCommand::RenderVideoFrame { .. }
            | ControlCommand::FinishVideoExport),
        ) => {
            deferred.push(cmd);
            false
        }
    }
}
