use std::sync::mpsc::Receiver;

use super::super::commands::{ControlCommand, RenderCommand};
use super::super::params::RenderParams;

/// 处理渲染命令
///
/// 先 drain 所有积压命令，然后如果没有 RenderCommand 则阻塞等待。
/// 返回 true 表示有渲染参数可以执行。
pub fn process_commands(
    command_receiver: &Receiver<RenderCommand>,
    latest_params: &mut Option<RenderParams>,
    should_shutdown: &mut bool,
) -> bool {
    // 先 drain 所有可用命令，丢弃过期的渲染参数
    while let Ok(cmd) = command_receiver.try_recv() {
        match cmd {
            RenderCommand::Render(params) => {
                *latest_params = Some(*params);
            }
            RenderCommand::Control(ControlCommand::Resize { width, height }) => {
                tracing::debug!("Render thread: resize to {}x{}", width, height);
            }
            RenderCommand::Control(ControlCommand::Shutdown) => {
                *should_shutdown = true;
            }
        }
    }

    // 如果已经收到 shutdown，直接返回
    if *should_shutdown {
        return false;
    }

    // 如果没有渲染参数，阻塞等待下一个命令
    if latest_params.is_none() {
        match command_receiver.recv() {
            Ok(RenderCommand::Render(params)) => {
                *latest_params = Some(*params);
            }
            Ok(RenderCommand::Control(ControlCommand::Shutdown)) => {
                *should_shutdown = true;
            }
            Ok(RenderCommand::Control(ControlCommand::Resize { width, height })) => {
                tracing::debug!("Render thread: resize to {}x{}", width, height);
            }
            Err(_) => {
                tracing::info!("Render thread: command channel closed");
                *should_shutdown = true;
            }
        }
    }

    latest_params.is_some()
}
