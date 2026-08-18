//! KDMAPI (Keppy's Direct MIDI API) 后端实现
//!
//! 使用 `libloading` 动态加载 OmniMIDI.dll，避免 nightly-only 的 kdmapi crate。
//! DLL 搜索策略继承自 [kdmapi-rs](https://github.com/khang06/kdmapi-rs)：
//! 1. 当前目录 / 系统 DLL 搜索路径（`LoadLibrary` 默认行为）
//! 2. `%WINDIR%\System32\OmniMIDI\OmniMIDI.dll`（标准安装路径）
//! 3. `%PROGRAMFILES%\OmniMIDI\OmniMIDI.dll`
//! 4. `%PROGRAMFILES(X86)%\OmniMIDI\OmniMIDI.dll`
//!
//! 参考文档：
//! - <https://github.com/KeppySoftware/OmniMIDI/blob/master/DeveloperContent/KDMAPI.md>
//! - <https://github.com/khang06/kdmapi-rs>

use libloading::Library;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;

use crate::{
    Api, Error, InputConnection, InputInfo, MidiInputCallback, OutputConnection, OutputInfo,
};

/// `libloading` 错误 → 我们的 Error 类型
impl From<libloading::Error> for crate::Error {
    fn from(e: libloading::Error) -> Self {
        Error::InitFailed(e.to_string())
    }
}

/// KDMAPI 全局实例（单例），确保每个进程只初始化一次
static KDMAPI_INSTANCE: Mutex<Option<Arc<KdmapiInner>>> = Mutex::new(None);

/// KDMAPI 符号表：从 OmniMIDI.dll 中动态加载的函数指针
///
/// ⚠️ 函数签名说明
///
/// | 函数 | 官方声明 | 返回是否有意义 |
/// |------|---------|--------------|
/// | `SendDirectData` | `VOID(WINAPI*)(DWORD)` — 不返回值 | ❌ 官方 doc 声明为 void |
/// | `SendDirectDataNoBuf` | `MMRESULT(WINAPI*)(DWORD)` 始终返回 NOERROR | ✅ 但始终 = NOERROR |
///
/// 所以 `send_direct_data` 声明为 `fn(u32)`（无返回值）。
/// 不要声明为 `fn(u32) -> u32`，否则读到的是 RAX 寄存器残留值 = UB。
struct Symbols {
    /// `bool ReturnKDMAPIVer(out Int32 Major, out Int32 Minor, out Int32 Build, out Int32 Revision);`
    return_kdmapi_ver: unsafe extern "system" fn(*mut i32, *mut i32, *mut i32, *mut i32) -> bool,
    /// `bool IsKDMAPIAvailable();`
    is_kdmapi_available: unsafe extern "system" fn() -> bool,
    /// `int InitializeKDMAPIStream();`
    initialize_kdmapi_stream: unsafe extern "system" fn() -> i32,
    /// `int TerminateKDMAPIStream();`
    terminate_kdmapi_stream: unsafe extern "system" fn() -> i32,
    /// `void SendDirectData(uint dwMsg);` — 不返回值（UB 若当作 u32 读）
    send_direct_data: unsafe extern "system" fn(u32),
}

/// 将 DLL 句柄与符号表打包进同一个 `Arc`，从类型层面杜绝"符号比库活得久"的问题。
///
/// `lib` 字段在语义上被使用（保持 DLL 加载），但 Rust 编译器可能因字段未被显式
/// 读取而发出 `dead_code` 警告，故此标注。
#[allow(dead_code)]
///
/// # 设计原理
///
/// 之前的设计中 `KdmapiInner` 持有 `_lib: Library` 和 `sym: Arc<Symbols>` 两个独立字段，
/// `KdmapiOutputConn` 则仅持有 `sym: Arc<Symbols>`。若所有 `KdmapiInner` 实例释放
/// （`KDMAPI_INSTANCE` 被清空或进程退出前部分析构），`Library` 先于 `Symbols` 被 drop
/// 导致 DLL 卸载，此时 `KdmapiOutputConn` 中的函数指针变成野指针，调用 = UB。
///
/// 通过将 `Library` 和 `Symbols` 绑进同一个 `Arc<KdmapiBundle>`，谁要用符号谁就持有
/// 这个 `Arc`——只要还有一个输出连接存活，DLL 就不会被卸载。析构顺序由 `Arc` 保证：
/// 最后一个引用释放时，先 drop `Symbols`（函数指针不再可用），再 drop `Library`。
///
/// # 注意事项
///
/// `Drop for KdmapiBundle` 中调用 FFI `terminate_kdmapi_stream`。
/// 在 `panic = "abort"` 下 abort 不走 drop，所以终止代码不应依赖此 drop 确保执行。
/// 正常退出路径下（走 `Drop`），此清理是安全的。
struct KdmapiBundle {
    lib: Library,
    sym: Symbols,
}

impl Drop for KdmapiBundle {
    fn drop(&mut self) {
        // 正常退出时终止 KDMAPI 流（panic=abort 下不走 drop，行为安全但不会执行此处）
        unsafe {
            (self.sym.terminate_kdmapi_stream)();
        }
        tracing::debug!("KDMAPI: 流已终止，DLL 即将卸载");
    }
}

/// KDMAPI 内部状态：持有打包的 DLL 句柄+符号表
struct KdmapiInner {
    bundle: Arc<KdmapiBundle>,
    version: String,
}

/// KDMAPI 实例（公开 API 入口）
pub struct Kdmapi {
    inner: Arc<KdmapiInner>,
}

/// KDMAPI 输出端口连接
///
/// 持有 `Arc<KdmapiBundle>` 而非 `Arc<Symbols>`，确保输出连接存活期间
/// DLL 不会被卸载（函数指针始终有效）。
struct KdmapiOutputConn {
    bundle: Arc<KdmapiBundle>,
}

// ---------------------------------------------------------------------------
// DLL 搜索策略
// ---------------------------------------------------------------------------

/// 生成尝试加载 OmniMIDI.dll 的路径列表
fn find_omnimidi_paths() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();

    // 1. 裸文件名 → 依赖系统 DLL 搜索路径
    paths.push(PathBuf::from("OmniMIDI.dll"));

    // 2. %WINDIR%\System32\OmniMIDI\OmniMIDI.dll（标准 64 位安装路径）
    if let Some(windir) = std::env::var_os("WINDIR") {
        let mut p = PathBuf::from(&windir);
        p.push("System32");
        p.push("OmniMIDI");
        p.push("OmniMIDI.dll");
        paths.push(p);

        // 3. %WINDIR%\SysWOW64\OmniMIDI\OmniMIDI.dll（32 位兼容路径）
        let mut p32 = PathBuf::from(&windir);
        p32.push("SysWOW64");
        p32.push("OmniMIDI");
        p32.push("OmniMIDI.dll");
        paths.push(p32);
    }

    // 4. %PROGRAMFILES%\OmniMIDI\OmniMIDI.dll
    if let Some(pf) = std::env::var_os("PROGRAMFILES") {
        let mut p = PathBuf::from(&pf);
        p.push("OmniMIDI");
        p.push("OmniMIDI.dll");
        paths.push(p);
    }

    // 5. %PROGRAMFILES(X86)%\OmniMIDI\OmniMIDI.dll
    if let Some(pfx86) = std::env::var_os("PROGRAMFILES(X86)") {
        let mut p = PathBuf::from(&pfx86);
        p.push("OmniMIDI");
        p.push("OmniMIDI.dll");
        paths.push(p);
    }

    paths
}

/// 尝试从给定的路径列表加载 Library，返回第一个成功的
///
/// # Safety
///
/// `Library::new` 调用 `dlopen`/`LoadLibrary`，在静态构造器抛出异常时可能 UB。
unsafe fn try_load_library(paths: &[PathBuf]) -> Result<Library, Error> {
    for path in paths {
        // SAFETY: Caller ensures no static constructors cause UB during loading.
        match unsafe { Library::new(path) } {
            Ok(lib) => {
                tracing::info!("KDMAPI: 成功加载 DLL: {:?}", path);
                return Ok(lib);
            }
            Err(e) => {
                tracing::debug!("KDMAPI: 无法从 {:?} 加载: {}", path, e);
            }
        }
    }

    Err(Error::InitFailed(format!(
        "OmniMIDI.dll 未找到。已尝试路径: {}。\
         请确认 OmniMIDI 已安装（https://github.com/KeppySoftware/OmniMIDI）",
        paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    )))
}

// ---------------------------------------------------------------------------
// Kdmapi 初始化
// ---------------------------------------------------------------------------

impl Kdmapi {
    /// 创建 KDMAPI 实例
    ///
    /// `path` 参数保留用于与 `ApiKind::Kdmapi` 的兼容性。
    /// 内部会使用更全面的搜索策略自动定位 OmniMIDI.dll。
    pub fn new(path: &Path) -> Result<Self, Error> {
        // 检查是否已经有初始化的实例
        if let Ok(guard) = KDMAPI_INSTANCE.lock()
            && let Some(instance) = guard.as_ref()
        {
            tracing::info!("KDMAPI 实例已存在，重用它");
            return Ok(Self {
                inner: Arc::clone(instance),
            });
        }

        // 构建搜索路径：用户指定的优先，后面是自动发现的路径
        let mut search_paths = vec![path.to_path_buf()];
        search_paths.extend(find_omnimidi_paths());

        // 去重（用户指定的可能和自动路径重复）
        search_paths.sort();
        search_paths.dedup();

        let lib = unsafe { try_load_library(&search_paths)? };

        unsafe {
            let sym = Symbols {
                return_kdmapi_ver: *lib.get(b"ReturnKDMAPIVer\0")?,
                is_kdmapi_available: *lib.get(b"IsKDMAPIAvailable\0")?,
                initialize_kdmapi_stream: *lib.get(b"InitializeKDMAPIStream\0")?,
                terminate_kdmapi_stream: *lib.get(b"TerminateKDMAPIStream\0")?,
                send_direct_data: *lib.get(b"SendDirectData\0")?,
            };

            // 1. 检查 KDMAPI 是否可用
            if !(sym.is_kdmapi_available)() {
                return Err(Error::InitFailed(
                    "KDMAPI 不可用（IsKDMAPIAvailable 返回 false）。\
                     请在 OmniMIDI 设置中启用 KDMAPI"
                        .into(),
                ));
            }

            // 2. 初始化 KDMAPI 音频流
            //    返回非零表示成功，零表示失败
            if (sym.initialize_kdmapi_stream)() == 0 {
                return Err(Error::InitFailed(
                    "初始化 KDMAPI 流失败。请确认 OmniMIDI 驱动程序运行正常".into(),
                ));
            }

            // 3. 获取驱动版本号
            let mut major = 0;
            let mut minor = 0;
            let mut patch = 0;
            let mut rev = 0;
            if !(sym.return_kdmapi_ver)(&mut major, &mut minor, &mut patch, &mut rev) {
                return Err(Error::InitFailed("获取 KDMAPI 版本失败".into()));
            }

            let bundle = Arc::new(KdmapiBundle { lib, sym });

            let inner = Arc::new(KdmapiInner {
                bundle,
                version: format!("v{major}.{minor}.{patch}.{rev}"),
            });

            // 保存到全局实例（后续 open_output 可以创建多个输出连接）
            if let Ok(mut guard) = KDMAPI_INSTANCE.lock() {
                *guard = Some(Arc::clone(&inner));
            }

            tracing::info!("KDMAPI 已初始化，版本: {}", inner.version);
            Ok(Self { inner })
        }
    }
}

// ---------------------------------------------------------------------------
// Api trait 实现
// ---------------------------------------------------------------------------

impl Api for Kdmapi {
    fn version(&self) -> Option<String> {
        Some(self.inner.version.clone())
    }

    fn inputs(&self) -> Result<Vec<InputInfo>, Error> {
        Ok(Vec::new())
    }

    fn outputs(&self) -> Result<Vec<OutputInfo>, Error> {
        Ok(vec![OutputInfo {
            id: 0,
            name: "KDMAPI".into(),
        }])
    }

    fn open_output(&self, id: u32) -> Result<Box<dyn OutputConnection>, Error> {
        if id != 0 {
            return Err(Error::DeviceNotFound(id));
        }
        Ok(Box::new(KdmapiOutputConn {
            bundle: Arc::clone(&self.inner.bundle),
        }))
    }

    fn open_input(
        &self,
        _id: u32,
        _callback: MidiInputCallback,
    ) -> Result<Box<dyn InputConnection>, Error> {
        Err(Error::InitFailed("KDMAPI 不支持 MIDI 输入".into()))
    }
}

// ---------------------------------------------------------------------------
// OutputConnection 实现
// ---------------------------------------------------------------------------

impl KdmapiOutputConn {
    fn send(&mut self, data: &[u8; 3]) -> Result<(), Error> {
        // MIDI 短消息格式（与 Windows midiOutShortMsg 兼容）：
        //   字节 0: 状态 (0x90 | channel)
        //   字节 1: 数据 1 (key)
        //   字节 2: 数据 2 (velocity)
        //   字节 3: 保留 (0)
        let word = u32::from_le_bytes([data[0], data[1], data[2], 0]);

        tracing::trace!(
            "KDMAPI: SendDirectData(0x{:08X})  [{:02X} {:02X} {:02X}]",
            word,
            data[0],
            data[1],
            data[2],
        );

        // SendDirectData 是 void 函数（不返回值），见 Symbols 文档说明
        // 消息"fail quietly"——不指示成功/失败，所以我们直接发送
        unsafe {
            (self.bundle.sym.send_direct_data)(word);
        }

        Ok(())
    }
}

impl OutputConnection for KdmapiOutputConn {
    fn send_raw(&mut self, data: [u8; 3]) -> Result<(), Error> {
        self.send(&data)
    }

    fn close(self: Box<Self>) {
        tracing::debug!("KDMAPI: 输出连接已关闭");
        // KDMAPI 连接不需要额外清理，TerminateKDMAPIStream 由 KdmapiBundle::drop 处理
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_omnimidi_paths_contains_bare_name() {
        let paths = find_omnimidi_paths();
        assert!(
            paths.iter().any(|p| p == Path::new("OmniMIDI.dll")),
            "路径列表应包含裸文件名"
        );
    }

    #[test]
    fn test_find_omnimidi_paths_unique() {
        let paths = find_omnimidi_paths();
        let mut sorted = paths.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(paths.len(), sorted.len(), "路径列表不应包含重复项");
    }
}
