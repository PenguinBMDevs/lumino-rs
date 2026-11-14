//! 原生运行时事件循环所有权：`RuntimeApp` + `run_with_event_loop`。
//!
//! 生产入口不再直接调用 `EventLoop::run_app`；事件循环由本 crate 持有并驱动，
//! 应用层只实现 [`RuntimeApp`]（与 winit `ApplicationHandler` 同构）。
//! 这是 Plan B 的第二步：先把「跑循环」的所有权从应用层收进运行时，后续再把
//! 窗口 / surface / 输入也收进来，应用层不再接触 winit 的窗口生命周期。

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::WindowId;

/// 应用层事件循环回调。
///
/// 与 winit `ApplicationHandler` 的方法签名保持一致，但去掉了 `new_events` /
/// `device_event` 等本应用不用的钩子；运行时负责把事件转发过来。
pub trait RuntimeApp<E = ()> {
    /// 窗口/应用恢复（桌面首帧、Android 回前台）。
    fn resumed(&mut self, event_loop: &ActiveEventLoop);
    /// 窗口事件。
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    );
    /// 自定义用户事件（device-lost / UI 唤醒等）；默认忽略。
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: E) {}
    /// 事件循环即将休眠。
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {}
    /// 进入后台（Android）。
    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {}
    /// 事件循环退出前。
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {}
}

/// 把 [`RuntimeApp`] 适配成 winit 的 `ApplicationHandler`。
struct RuntimeHost<A> {
    app: A,
}

impl<A, E> ApplicationHandler<E> for RuntimeHost<A>
where
    A: RuntimeApp<E>,
    E: 'static,
{
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.app.resumed(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        self.app.window_event(event_loop, window_id, event);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: E) {
        self.app.user_event(event_loop, event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.app.about_to_wait(event_loop);
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.app.suspended(event_loop);
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        self.app.exiting(event_loop);
    }
}

/// 用给定事件循环运行应用。
///
/// `build` 在 `run_app` 之前调用，用于注册 device-lost / UI waker 等
/// `EventLoopProxy`——proxy 必须在循环开始前创建。
pub fn run_with_event_loop<A, E>(
    event_loop: EventLoop<E>,
    build: impl FnOnce(&EventLoop<E>) -> A,
) -> Result<(), winit::error::EventLoopError>
where
    A: RuntimeApp<E>,
    E: 'static,
{
    // 默认休眠；应用可在 `about_to_wait` 里按需改成 WaitUntil（播放 / 导出心跳）。
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);
    let app = build(&event_loop);
    event_loop.run_app(&mut RuntimeHost { app })
}

/// 创建默认事件循环并运行应用。
///
/// Android / Linux 需要平台自定义入口时用 [`run_with_event_loop`]。
pub fn run<A, E>(build: impl FnOnce(&EventLoop<E>) -> A) -> Result<(), winit::error::EventLoopError>
where
    A: RuntimeApp<E>,
    E: 'static,
{
    let event_loop = EventLoop::<E>::with_user_event().build()?;
    run_with_event_loop(event_loop, build)
}
