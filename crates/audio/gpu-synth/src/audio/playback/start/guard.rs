//! 启动失败的线程回收守卫。
//!
//! `start` 在「渲染线程已起、音频流未就绪」的窗口里可能出错；守卫负责在任意
//! `?` 提前返回时停线程并 join，成功时 `disarm` 交出句柄。

use super::super::*;

/// 启动失败时回收已启动线程；成功交还句柄后不再干预播放。
pub(super) struct StartupThreadGuard {
    stop_flag: Arc<AtomicBool>,
    stop_tx: mpsc::Sender<()>,
    render_thread: Option<JoinHandle<()>>,
    stream_owner: Option<JoinHandle<()>>,
    armed: bool,
}

impl StartupThreadGuard {
    pub(super) fn new(
        stop_flag: Arc<AtomicBool>,
        stop_tx: mpsc::Sender<()>,
        render_thread: JoinHandle<()>,
    ) -> Self {
        Self {
            stop_flag,
            stop_tx,
            render_thread: Some(render_thread),
            stream_owner: None,
            armed: true,
        }
    }

    /// 挂上 stream owner 线程；之后 `drop` 会一并回收它。
    pub(super) fn attach_stream_owner(&mut self, handle: JoinHandle<()>) {
        self.stream_owner = Some(handle);
    }

    pub(super) fn disarm(mut self) -> (Option<JoinHandle<()>>, Option<JoinHandle<()>>) {
        self.armed = false;
        (self.render_thread.take(), self.stream_owner.take())
    }
}

impl Drop for StartupThreadGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        self.stop_flag.store(true, Ordering::Relaxed);
        let _ = self.stop_tx.send(());
        if let Some(handle) = self.stream_owner.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.render_thread.take() {
            let _ = handle.join();
        }
    }
}
