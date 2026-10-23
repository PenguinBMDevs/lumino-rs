//! Windows Job Object：父进程崩溃时连带终止探针子进程。
//!
//! 为什么必须做：探针挂起时持有驱动 / loader 锁。若父进程先崩溃（或被强杀），
//! 孤儿探针会让**重启后的新实例**继续被拖死——这比「没做隔离」更糟，因为用户
//! 以为问题已经修好了。`KILL_ON_JOB_CLOSE` 让「父进程句柄关闭」等价于「杀掉子树」。

use std::ffi::c_void;
use std::os::windows::io::AsRawHandle;
use std::process::Child;
use std::ptr;

use winapi::um::handleapi::CloseHandle;
use winapi::um::jobapi2::{AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject};
use winapi::um::winnt::{
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectExtendedLimitInformation,
};

/// 持有期间保证「父进程一死就杀子进程」的 Job 句柄（RAII）。
pub(super) struct KillOnCloseJob(*mut c_void);

impl KillOnCloseJob {
    /// 创建 Job 并把子进程绑进去。
    ///
    /// 任何一步失败都返回 `None`（调用方只记日志继续）：Job 只是「父进程崩溃」的
    /// 兜底，超时强杀不依赖它。已处于其他 Job（CI / 启动器）中的子进程可能绑定失败，
    /// 这是预期内的降级。
    pub(super) fn assign(child: &Child) -> Option<Self> {
        // SAFETY: 句柄由 Self 独占并在 Drop 中关闭一次；构造失败的分支各自负责关闭。
        unsafe {
            let job = CreateJobObjectW(ptr::null_mut(), ptr::null());
            if job.is_null() {
                return None;
            }

            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let configured = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &mut info as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION as *mut c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if configured == 0 {
                CloseHandle(job);
                return None;
            }

            if AssignProcessToJobObject(job, child.as_raw_handle()) == 0 {
                CloseHandle(job);
                return None;
            }

            Some(Self(job))
        }
    }
}

impl Drop for KillOnCloseJob {
    fn drop(&mut self) {
        // SAFETY: 仅关闭本结构持有的句柄，且只关闭一次（Drop 后不再使用）。
        unsafe {
            CloseHandle(self.0);
        }
    }
}
