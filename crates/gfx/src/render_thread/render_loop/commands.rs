use std::sync::mpsc::Receiver;

use super::super::commands::{ControlCommand, RenderCommand};
use super::super::params::RenderParams;

/// 处理渲染命令
///
/// 先 drain 所有积压命令，然后如果没有 RenderCommand 则阻塞等待。
/// 返回 true 表示有渲染参数可以执行。
///
/// 洋葱皮控制命令（Generate/Dispose）需要 device/queue 上下文，无法在此处理，
/// 因此收集到 `deferred` 供主循环在拥有 GPU 资源时处理。
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

    // 如果没有渲染参数，阻塞等待下一个命令
    if latest_params.is_none() {
        match command_receiver.recv() {
            Ok(cmd) => classify_command(cmd, latest_params, should_shutdown, deferred),
            Err(_) => {
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
        // 洋葱皮命令需要 GPU 资源上下文，延迟到主循环处理
        RenderCommand::Control(
            onion @ (ControlCommand::GenerateOnionSkin { .. }
            | ControlCommand::DisposeOnionSkin
            | ControlCommand::GenerateHiResOnionSkin { .. }
            | ControlCommand::DisposeHiResOnionSkin
            | ControlCommand::RegenerateHiResTrack { .. }),
        ) => {
            deferred.push(onion);
        }
    }
}
