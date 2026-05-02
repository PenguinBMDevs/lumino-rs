use std::sync::mpsc::Receiver;

use super::super::commands::{ControlCommand, RenderCommand};
use super::super::params::RenderParams;

pub fn process_commands(
    command_receiver: &Receiver<RenderCommand>,
    latest_params: &mut Option<RenderParams>,
    should_shutdown: &mut bool,
) {
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
}
