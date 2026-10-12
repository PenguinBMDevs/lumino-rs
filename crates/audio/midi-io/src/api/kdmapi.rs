//! **DRI（单人负责）**: lumino-audio 音频 I/O 负责人。所有 `libloading`/`extern "system"` FFI 变更须经其 review。
//!
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

use lumino_core::{tr, trf};

use crate::{
    Api, Error, InputConnection, InputInfo, MidiInputCallback, OutputConnection, OutputInfo,
    PlaybackOutput, SynthControl,
};

/// KDMAPI 全局实例（单例），确保每个进程只初始化一次
static KDMAPI_INSTANCE: Mutex<Option<Arc<KdmapiInner>>> = Mutex::new(None);

/// 版本符号存在但调用失败时的降级版本串。
const VERSION_UNKNOWN: &str = "unknown";
/// `ReturnKDMAPIVer` 符号缺失（OmniMIDI v2+ 不再导出）时的降级版本串。
const VERSION_UNKNOWN_V2: &str = "unknown (OmniMIDI v2+)";

/// v1 版本串格式（行为与历史一致，独立成函数便于单测）。
fn format_version(major: i32, minor: i32, patch: i32, rev: i32) -> String {
    format!("v{major}.{minor}.{patch}.{rev}")
}

/// KDMAPI 符号表：从 OmniMIDI.dll 中动态加载的函数指针
///
/// # OmniMIDI v1 / v2 差异（v2 = 15.0+，<https://github.com/KaleidonKep99/OmniMIDIv2>）
///
/// | 符号 | v1 | v2 | Lumino 处理 |
/// |------|----|----|-------------|
/// | `ReturnKDMAPIVer` | 导出 | **不再导出**（v2 全仓零引用；其 winelib shim 同名符号亦 dlsym 不到） | **可选**：缺失 → 版本降级 [`VERSION_UNKNOWN_V2`]，不阻断连接 |
/// | `IsKDMAPIAvailable` / `InitializeKDMAPIStream` / `TerminateKDMAPIStream` / `SendDirectData` | 导出 | 保留（签名二进制兼容） | **必需**：缺失 → 初始化失败并提示"可能为 v2 / 非 KDMAPI 构建" |
/// | `LoadCustomSoundFontsList` / `PrepareLongData` | 导出 | 退化为 stub | Lumino 未使用 |
///
/// v2 同时移除的 `GetDriverDebugInfo` / `NoFeedbackMode` / `PoweredByKeppy` 与
/// 全部 WinMM passthrough 导出，Lumino 均未使用，无影响。
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
    ///
    /// 可选符号：OmniMIDI v2 不再导出（见上方差异表）。
    return_kdmapi_ver:
        Option<unsafe extern "system" fn(*mut i32, *mut i32, *mut i32, *mut i32) -> bool>,
    /// `bool IsKDMAPIAvailable();`
    is_kdmapi_available: unsafe extern "system" fn() -> bool,
    /// `int InitializeKDMAPIStream();`
    initialize_kdmapi_stream: unsafe extern "system" fn() -> i32,
    /// `int TerminateKDMAPIStream();`
    terminate_kdmapi_stream: unsafe extern "system" fn() -> i32,
    /// `void SendDirectData(uint dwMsg);` — 不返回值（UB 若当作 u32 读）
    send_direct_data: unsafe extern "system" fn(u32),
}

/// 符号来源抽象：真实实现包一层 [`libloading::Library`]；测试注入 fake 桩
/// （不依赖真实 DLL 即可覆盖"符号缺失降级"路径）。
trait KdmapiLib {
    /// 可选符号：失败返回 `None`（v2 降级路径）。
    fn return_kdmapi_ver(
        &self,
    ) -> Option<unsafe extern "system" fn(*mut i32, *mut i32, *mut i32, *mut i32) -> bool>;
    fn is_kdmapi_available(&self) -> Option<unsafe extern "system" fn() -> bool>;
    fn initialize_kdmapi_stream(&self) -> Option<unsafe extern "system" fn() -> i32>;
    fn terminate_kdmapi_stream(&self) -> Option<unsafe extern "system" fn() -> i32>;
    fn send_direct_data(&self) -> Option<unsafe extern "system" fn(u32)>;
}

/// 真实符号来源：`libloading::Library` 的薄包装。
struct RealKdmapiLib<'a>(&'a Library);

impl KdmapiLib for RealKdmapiLib<'_> {
    fn return_kdmapi_ver(
        &self,
    ) -> Option<unsafe extern "system" fn(*mut i32, *mut i32, *mut i32, *mut i32) -> bool> {
        // SAFETY: 符号名与 `Symbols` 声明的签名匹配（v1/v2 二进制兼容，见结构文档）；
        // 拷贝出的函数指针只在 `KdmapiBundle`（持有 `Library`）存活期间被调用。
        unsafe { self.0.get(b"ReturnKDMAPIVer\0").ok().map(|s| *s) }
    }

    fn is_kdmapi_available(&self) -> Option<unsafe extern "system" fn() -> bool> {
        // SAFETY: 同 `return_kdmapi_ver`（签名匹配 + 随 `KdmapiBundle` 存活）。
        unsafe { self.0.get(b"IsKDMAPIAvailable\0").ok().map(|s| *s) }
    }

    fn initialize_kdmapi_stream(&self) -> Option<unsafe extern "system" fn() -> i32> {
        // SAFETY: 同 `return_kdmapi_ver`（签名匹配 + 随 `KdmapiBundle` 存活）。
        unsafe { self.0.get(b"InitializeKDMAPIStream\0").ok().map(|s| *s) }
    }

    fn terminate_kdmapi_stream(&self) -> Option<unsafe extern "system" fn() -> i32> {
        // SAFETY: 同 `return_kdmapi_ver`（签名匹配 + 随 `KdmapiBundle` 存活）。
        unsafe { self.0.get(b"TerminateKDMAPIStream\0").ok().map(|s| *s) }
    }

    fn send_direct_data(&self) -> Option<unsafe extern "system" fn(u32)> {
        // SAFETY: 同 `return_kdmapi_ver`（签名匹配 + 随 `KdmapiBundle` 存活）。
        unsafe { self.0.get(b"SendDirectData\0").ok().map(|s| *s) }
    }
}

/// 解析符号表：缺必需符号返回缺失名单（按检查顺序）；`ReturnKDMAPIVer`
/// 缺失不视为错误（OmniMIDI v2 降级路径）。
fn resolve_symbols(lib: &dyn KdmapiLib) -> Result<Symbols, Vec<&'static str>> {
    // 一次绑定：齐全直接取用，缺任意项则收集全部缺失名后统一返回。
    let (is_kdmapi_available, initialize_kdmapi_stream, terminate_kdmapi_stream, send_direct_data) =
        match (
            lib.is_kdmapi_available(),
            lib.initialize_kdmapi_stream(),
            lib.terminate_kdmapi_stream(),
            lib.send_direct_data(),
        ) {
            (Some(a), Some(b), Some(c), Some(d)) => (a, b, c, d),
            (a, b, c, d) => {
                let mut missing: Vec<&'static str> = Vec::new();
                if a.is_none() {
                    missing.push("IsKDMAPIAvailable");
                }
                if b.is_none() {
                    missing.push("InitializeKDMAPIStream");
                }
                if c.is_none() {
                    missing.push("TerminateKDMAPIStream");
                }
                if d.is_none() {
                    missing.push("SendDirectData");
                }
                return Err(missing);
            }
        };

    Ok(Symbols {
        return_kdmapi_ver: lib.return_kdmapi_ver(),
        is_kdmapi_available,
        initialize_kdmapi_stream,
        terminate_kdmapi_stream,
        send_direct_data,
    })
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

    Err(Error::InitFailed(trf!(
        "OmniMIDI.dll 未找到。已尝试路径: {paths}。请确认 OmniMIDI 已安装（https://github.com/KeppySoftware/OmniMIDI）",
        paths = paths
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

        // 符号解析：`ReturnKDMAPIVer` 可选（OmniMIDI v2 不再导出），其余 4 个必需；
        // 缺必需符号区分于"加载失败"，给出可操作的错误提示（v2 / 非 KDMAPI 构建）。
        let sym = match resolve_symbols(&RealKdmapiLib(&lib)) {
            Ok(sym) => sym,
            Err(missing) => {
                let list = missing.join(", ");
                tracing::error!("KDMAPI: 关键符号缺失: {list}");
                return Err(Error::InitFailed(trf!(
                    "KDMAPI 关键符号缺失: {list}。可能为 OmniMIDI v2（已移除部分旧版符号）或该 DLL 非 KDMAPI 构建；请升级或重装 OmniMIDI 后重试",
                    list = list
                )));
            }
        };

        unsafe {
            // 1. 检查 KDMAPI 是否可用
            if !(sym.is_kdmapi_available)() {
                tracing::warn!("KDMAPI: IsKDMAPIAvailable 返回 false，无法连接");
                return Err(Error::InitFailed(
                    tr!("KDMAPI 不可用（IsKDMAPIAvailable 返回 false）。请在 OmniMIDI 设置中启用 KDMAPI").into(),
                ));
            }

            // 2. 初始化 KDMAPI 音频流
            //    返回非零表示成功，零表示失败
            if (sym.initialize_kdmapi_stream)() == 0 {
                tracing::warn!("KDMAPI: InitializeKDMAPIStream 返回 0，初始化失败");
                return Err(Error::InitFailed(
                    tr!(
                        "初始化 KDMAPI 流失败（InitializeKDMAPIStream 返回 0）。若使用 OmniMIDI v2，请检查其 settings.json 的 renderer/合成器配置（指向未编译的模块会导致失败）与音色库选择；确认驱动状态后重试"
                    )
                    .into(),
                ));
            }

            // 3. 获取驱动版本号（v2 起 `ReturnKDMAPIVer` 不再导出 → 降级字符串，不阻断连接）
            let version = match sym.return_kdmapi_ver {
                Some(return_ver) => {
                    let mut major = 0;
                    let mut minor = 0;
                    let mut patch = 0;
                    let mut rev = 0;
                    if return_ver(&mut major, &mut minor, &mut patch, &mut rev) {
                        format_version(major, minor, patch, rev)
                    } else {
                        tracing::warn!(
                            "KDMAPI: ReturnKDMAPIVer 调用失败，版本降级为 {VERSION_UNKNOWN}"
                        );
                        VERSION_UNKNOWN.to_string()
                    }
                }
                None => {
                    tracing::info!(
                        "KDMAPI: 未导出 ReturnKDMAPIVer（OmniMIDI v2+），版本降级为 {VERSION_UNKNOWN_V2}"
                    );
                    VERSION_UNKNOWN_V2.to_string()
                }
            };

            let bundle = Arc::new(KdmapiBundle { lib, sym });

            let inner = Arc::new(KdmapiInner { bundle, version });

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

    fn open_output(&self, id: u32) -> Result<Box<dyn PlaybackOutput>, Error> {
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
        Err(Error::InitFailed(tr!("KDMAPI 不支持 MIDI 输入").into()))
    }
}

/// REND-002：显式加入能力扩展（空 impl = 默认：不支持多端口布局）。
impl SynthControl for Kdmapi {}

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

/// REND-002：显式加入播放能力扩展（空 impl = 默认：模态 no-op / 16 通道踏板释放）。
impl PlaybackOutput for KdmapiOutputConn {}

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

    // ── 符号解析层（fake 库桩，不依赖真实 DLL）──

    unsafe extern "system" fn stub_return_ver(
        _: *mut i32,
        _: *mut i32,
        _: *mut i32,
        _: *mut i32,
    ) -> bool {
        true
    }
    unsafe extern "system" fn stub_available() -> bool {
        true
    }
    unsafe extern "system" fn stub_init() -> i32 {
        1
    }
    unsafe extern "system" fn stub_term() -> i32 {
        0
    }
    unsafe extern "system" fn stub_send(_: u32) {}

    /// fake 符号来源：`missing` 中的符号返回 `None`，其余返回桩函数。
    struct FakeLib {
        missing: &'static [&'static str],
    }

    impl KdmapiLib for FakeLib {
        fn return_kdmapi_ver(
            &self,
        ) -> Option<unsafe extern "system" fn(*mut i32, *mut i32, *mut i32, *mut i32) -> bool>
        {
            (!self.missing.contains(&"ReturnKDMAPIVer")).then_some(stub_return_ver)
        }
        fn is_kdmapi_available(&self) -> Option<unsafe extern "system" fn() -> bool> {
            (!self.missing.contains(&"IsKDMAPIAvailable")).then_some(stub_available)
        }
        fn initialize_kdmapi_stream(&self) -> Option<unsafe extern "system" fn() -> i32> {
            (!self.missing.contains(&"InitializeKDMAPIStream")).then_some(stub_init)
        }
        fn terminate_kdmapi_stream(&self) -> Option<unsafe extern "system" fn() -> i32> {
            (!self.missing.contains(&"TerminateKDMAPIStream")).then_some(stub_term)
        }
        fn send_direct_data(&self) -> Option<unsafe extern "system" fn(u32)> {
            (!self.missing.contains(&"SendDirectData")).then_some(stub_send)
        }
    }

    /// OmniMIDI v2 降级路径：缺 `ReturnKDMAPIVer` 时解析成功且该符号为 None。
    #[test]
    fn test_resolve_symbols_degrades_when_version_symbol_missing() {
        let lib = FakeLib {
            missing: &["ReturnKDMAPIVer"],
        };
        let sym = resolve_symbols(&lib).expect("缺版本符号不应导致解析失败");
        assert!(
            sym.return_kdmapi_ver.is_none(),
            "v2 缺版本符号应为 None（降级路径）"
        );
    }

    /// 缺必需符号：返回准确名单（供 v2 / 非 KDMAPI 构建的可操作提示）。
    #[test]
    fn test_resolve_symbols_reports_missing_required() {
        let lib = FakeLib {
            missing: &["IsKDMAPIAvailable", "SendDirectData"],
        };
        let missing = resolve_symbols(&lib)
            .err()
            .expect("缺必需符号应报错（返回 Err(缺失名单)）");
        assert_eq!(missing, vec!["IsKDMAPIAvailable", "SendDirectData"]);
    }

    /// 全齐（OmniMIDI v1 正常路径）：所有符号就位。
    #[test]
    fn test_resolve_symbols_all_present() {
        let lib = FakeLib { missing: &[] };
        let sym = resolve_symbols(&lib).expect("符号齐全应解析成功");
        assert!(sym.return_kdmapi_ver.is_some());
    }

    /// 版本串契约：v1 路径 `v{major}.{minor}.{patch}.{rev}`；降级常量固定。
    #[test]
    fn test_version_strings() {
        assert_eq!(format_version(2, 4, 1, 0), "v2.4.1.0");
        assert_eq!(VERSION_UNKNOWN_V2, "unknown (OmniMIDI v2+)");
        assert_eq!(VERSION_UNKNOWN, "unknown");
    }
}
