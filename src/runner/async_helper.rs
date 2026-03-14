use lumino_core::event;

/// 异步任务处理辅助函数
/// 执行异步操作并根据结果发射事件
pub async fn run_async_task<T, F, Success, Error>(
    task: F,
    success_event: Success,
    error_event: Error,
) where
    F: std::future::Future<Output = Result<T, String>>,
    Success: FnOnce(T) -> event::Event,
    Error: FnOnce(String) -> event::Event,
{
    match task.await {
        Ok(result) => {
            event::emit(success_event(result));
        }
        Err(e) => {
            event::emit(error_event(e));
        }
    }
}

/// 带进度消息的异步任务处理辅助函数
pub async fn run_async_task_with_progress<T, F, Success, Error>(
    task: F,
    progress_message: &str,
    progress_value: f64,
    success_event: Success,
    error_event: Error,
) where
    F: std::future::Future<Output = Result<T, String>>,
    Success: FnOnce(T) -> event::Event,
    Error: FnOnce(String) -> event::Event,
{
    lumino_core::midi::loader::send_progress_message(progress_message, progress_value);

    match task.await {
        Ok(result) => {
            event::emit(success_event(result));
        }
        Err(e) => {
            event::emit(error_event(e));
        }
    }
}
