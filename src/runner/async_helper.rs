use lumino_ui::event;

/// 异步任务处理辅助函数
/// 执行异步操作并根据结果发射事件
pub async fn run_async_task<T, F, Success, Error, E>(
    task: F,
    success_event: Success,
    error_event: Error,
) where
    F: std::future::Future<Output = Result<T, E>>,
    Success: FnOnce(T) -> event::Event,
    Error: FnOnce(String) -> event::Event,
    E: std::fmt::Display,
{
    match task.await {
        Ok(result) => {
            event::emit(success_event(result));
        }
        Err(e) => {
            event::emit(error_event(e.to_string()));
        }
    }
}
