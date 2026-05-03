//! 测试模式 FPS 监测模块

use std::time::{Duration, Instant};

use winit::event_loop::ActiveEventLoop;

use crate::runner::inner::RunnerInner;

impl RunnerInner {
    /// 处理测试模式 FPS 监测
    pub(crate) fn handle_test_mode_fps(&mut self, event_loop: &ActiveEventLoop) {
        let Some(test_state) = &mut self.test_state.test_mode_state else {
            return;
        };

        if !test_state.active {
            return;
        }

        // 初始化测试开始时间
        if test_state.start_time.is_none() {
            test_state.start_time = Some(Instant::now());
            test_state.last_fps_update = Some(Instant::now());
            tracing::info!("FPS 测试开始");
        }

        test_state.frame_count += 1;
        let now = Instant::now();

        if let Some(last) = test_state.last_fps_update {
            let elapsed = now.duration_since(last);
            if elapsed.as_millis() >= 100 {
                let fps = test_state.frame_count as f32 / elapsed.as_secs_f32();
                test_state.fps_samples.push(fps);
                test_state.frame_count = 0;
                test_state.last_fps_update = Some(now);

                tracing::info!(
                    "FPS: {:.1} (samples: {})",
                    fps,
                    test_state.fps_samples.len()
                );

                // 检查是否达到测试时长
                if let Some(duration) = test_state.duration {
                    let should_exit = test_state
                        .start_time
                        .map(|start| now.duration_since(start) >= Duration::from_secs(duration))
                        .unwrap_or(false);

                    if should_exit {
                        let avg_fps = test_state.fps_samples.iter().sum::<f32>()
                            / test_state.fps_samples.len() as f32;
                        tracing::info!("================================");
                        tracing::info!("FPS 测试完成");
                        tracing::info!("平均 FPS: {:.2}", avg_fps);
                        tracing::info!("采样次数：{}", test_state.fps_samples.len());
                        tracing::info!("================================");
                        event_loop.exit();
                    }
                }
            }
        }
    }
}
