use std::sync::mpsc::Receiver;
use std::time::Duration;

use super::super::commands::{ControlCommand, RenderCommand};
use super::super::params::RenderParams;

/// 命令接收阻塞超时：防止渲染线程在 `recv()` 上无限阻塞，
/// 导致后台 hires 贴图 channel 不被消费（死锁）。
/// 超时后醒来检查 hires channel + 渲染帧，然后重新进入等待。
const COMMAND_RECV_TIMEOUT: Duration = Duration::from_millis(16);

/// 处理渲染命令
///
/// 先 drain 所有积压命令，然后如果没有 RenderCommand 则阻塞等待。
/// 返回 true 表示有渲染参数可以执行。
///
/// 洋葱皮控制命令（Generate/Dispose）需要 device/queue 上下文，无法在此处理，
/// 因此收集到 `deferred` 供主循环在拥有 GPU 资源时处理。
///
/// 使用 `recv_timeout` 而非 `recv`：防止渲染线程在无命令时无限阻塞，
/// 确保定期返回主循环以消费后台 hires 贴图 channel（避免死锁）。
pub fn process_commands(
    command_receiver: &Receiver<RenderCommand>,
    latest_params: &mut Option<RenderParams>,
    should_shutdown: &mut bool,
    deferred: &mut Vec<ControlCommand>,
) -> bool {
    // 先 drain 所有可用命令，丢弃过期的渲染参数
    while let Ok(cmd) = command_receiver.try_recv() {
        classify_command(cmd, latest_params, should_shutdown, deferred);
    }

    // 如果已经收到 shutdown，直接返回
    if *should_shutdown {
        return false;
    }

    // 如果没有渲染参数，限时等待下一个命令（而非无限阻塞）
    // 超时后返回主循环，使其能消费 hires channel 等后台数据
    if latest_params.is_none() {
        match command_receiver.recv_timeout(COMMAND_RECV_TIMEOUT) {
            Ok(cmd) => classify_command(cmd, latest_params, should_shutdown, deferred),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                tracing::info!("Render thread: command channel closed");
                *should_shutdown = true;
            }
        }
    }

    latest_params.is_some()
}

/// 将单条命令分类处理
fn classify_command(
    cmd: RenderCommand,
    latest_params: &mut Option<RenderParams>,
    should_shutdown: &mut bool,
    deferred: &mut Vec<ControlCommand>,
) {
    match cmd {
        RenderCommand::Render(params) => {
            *latest_params = Some(*params);
        }
        RenderCommand::Control(ControlCommand::Shutdown) => {
            *should_shutdown = true;
        }
        RenderCommand::Control(ControlCommand::Resize { width, height }) => {
            tracing::debug!("Render thread: resize to {}x{}", width, height);
        }
        // 洋葱皮控制命令需要 GPU 资源上下文，延迟到主循环处理
        RenderCommand::Control(
            cmd @ (ControlCommand::GenerateHiResOnionSkin { .. }
            | ControlCommand::DisposeHiResOnionSkin
            | ControlCommand::RegenerateHiResTrack { .. }
            | ControlCommand::ShowHiResDirtyOverlay { .. }),
        ) => {
            deferred.push(cmd);
        }
        // 视频导出命令需要 GPU 资源上下文，延迟到主循环处理
        RenderCommand::Control(
            cmd @ (ControlCommand::StartVideoExport { .. }
            | ControlCommand::RenderVideoFrame { .. }
            | ControlCommand::FinishVideoExport),
        ) => {
            deferred.push(cmd);
        }
    }
}
