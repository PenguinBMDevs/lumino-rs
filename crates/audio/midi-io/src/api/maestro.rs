//! Maestro 合成器宿主后端（AUD-009 Phase 1）。
//!
//! 通过 `maestro-api-rs` 运行时加载 Maestro 独立库；Lumino 只做连接层，
//! SoundFont、合成参数与音频设备由 Maestro 的用户 profile（按 app_id）管理。
//!
//! # 时间语义（Phase 1）
//!
//! 本阶段不做时间戳前视：所有事件用 [`TIME_NOW`] 发送，依赖 Maestro 默认的
//! `auto_timestamp`（按到达时间盖戳）获得接近播放线程节奏的时序。per-event tick、
//! 小前视窗与 `send_long` 批量留待 AUD-010。
//!
//! # 多端口（REND-002）
//!
//! `num_ports = min(max_port, 15) + 1`；全局通道 `ch` 映射
//! `port = ch / 16`、`channel = ch & 0x0F`。改布局必须重开 Maestro 引擎，
//! 已创建的输出连接通过共享句柄 + generation 热替换跟随新引擎。

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

#[cfg(not(target_os = "android"))]
use maestro_api_rs::LoadError;
use maestro_api_rs::{
    App, Error as MaestroError, ErrorKind, Info, LogLevel, Maestro as MaestroLib, Notification,
    Realtime, RealtimeOptions,
};
use parking_lot::{Mutex, RwLock};

use crate::api::layout::{LayoutAction, layout_action};
use crate::{
    Api, Error, InputConnection, InputInfo, MidiInputCallback, OutputInfo, PlaybackOutput,
    SynthControl,
};

mod output;
mod stats;

pub use output::MaestroOutputConn;
pub use stats::{MaestroStats, MaestroStatsReader};

/// Lumino 在 Maestro 用户配置中的稳定 app_id（reverse-DNS）。
pub const APP_ID: &str = "dev.penguin.lumino";

/// Maestro 原生端口上限（MIDI 1.0，1-16）。
const MAX_PORTS: u8 = 16;

const FLAG_ENGINE_STOPPED: u8 = 1 << 0;
const FLAG_ENGINE_DISABLED: u8 = 1 << 1;
const FLAG_SETTINGS_CHANGED: u8 = 1 << 2;

/// Maestro 合成器后端。
pub struct Maestro {
    shared: Arc<MaestroShared>,
    version: String,
}

struct MaestroShared {
    lib: MaestroLib,
    app_id: String,
    /// 当前引擎；布局重建时整体替换（旧引擎在最后一个引用释放后关闭）。
    engine: RwLock<Option<Arc<Realtime>>>,
    /// 最近一次 `get_info` 结果（实际生效的端口/调度窗口）。
    info: RwLock<Option<Info>>,
    /// 当前请求的最大端口（0 = 单端口）。
    max_port: AtomicU8,
    /// 引擎替换代数；输出连接据此决定是否重新读取。
    generation: AtomicU64,
    /// `Notification` 标志位（引擎停止/禁用/设置变更）。
    flags: AtomicU8,
    /// 待展示给用户的引擎通知（取走即清，避免刷屏）。
    notice: Mutex<Option<String>>,
}

fn normalize_max_port(max_port: u8) -> u8 {
    max_port.min(MAX_PORTS - 1)
}

fn num_ports(max_port: u8) -> u8 {
    normalize_max_port(max_port) + 1
}

fn log_maestro(level: LogLevel, message: &str) {
    match level {
        LogLevel::Error => tracing::error!("Maestro: {message}"),
        LogLevel::Warn => tracing::warn!("Maestro: {message}"),
        _ => tracing::debug!("Maestro: {message}"),
    }
}

#[cfg(not(target_os = "android"))]
fn map_load_error(error: LoadError) -> Error {
    Error::InitFailed(format!(
        "Maestro 未安装或无法加载（{error}）。请从 https://dimms.gr/maestro 安装 Maestro 后重试"
    ))
}

fn map_maestro_error(error: MaestroError) -> Error {
    let message = match error.kind() {
        ErrorKind::Disabled => "Maestro 已禁用本应用配置；请在 Maestro 设置中启用",
        ErrorKind::Unavailable => "Maestro 合成器或音频设备无法启动；请检查 Maestro 输出设备设置",
        ErrorKind::Unsupported => {
            "当前 Maestro 版本过旧，缺少 maestro.realtime/1 接口；请升级 Maestro"
        }
        ErrorKind::InvalidArg => "Maestro 拒绝了本次调用的参数",
        ErrorKind::NotRunning => "Maestro 引擎未运行（可能正在重建）",
        ErrorKind::Internal => "Maestro 内部错误（可能捕获到 panic）",
        _ => "Maestro 调用失败",
    };
    Error::InitFailed(format!("{message}（{}）", error.message()))
}

impl MaestroShared {
    fn new(lib: MaestroLib, app_id: &str, max_port: u8) -> Result<Arc<Self>, Error> {
        let shared = Arc::new(Self {
            lib,
            app_id: app_id.to_string(),
            engine: RwLock::new(None),
            info: RwLock::new(None),
            max_port: AtomicU8::new(normalize_max_port(max_port)),
            generation: AtomicU64::new(0),
            flags: AtomicU8::new(0),
            notice: Mutex::new(None),
        });
        shared.install_engine(max_port)?;
        Ok(shared)
    }

    /// 重新打开引擎并热替换；失败时旧引擎保持不动。
    fn install_engine(self: &Arc<Self>, max_port: u8) -> Result<(), Error> {
        let target = normalize_max_port(max_port);
        let weak = Arc::downgrade(self);
        let app = App::new(self.app_id.clone())
            .name("Lumino")
            .on_log(log_maestro);
        let options = RealtimeOptions::new()
            .num_ports(num_ports(target))
            .on_notify(move |notification| {
                if let Some(shared) = weak.upgrade() {
                    shared.on_notify(notification);
                }
            });

        let realtime = self
            .lib
            .open_realtime(&app, options)
            .map_err(map_maestro_error)?;
        let info = realtime.info().map_err(map_maestro_error)?;

        *self.engine.write() = Some(Arc::new(realtime));
        *self.info.write() = Some(info);
        self.max_port.store(target, Ordering::Release);
        self.flags.store(0, Ordering::Release);
        self.generation.fetch_add(1, Ordering::Release);
        tracing::info!(
            "Maestro: 引擎已就绪（num_ports={}，sample_rate={}，block={}us，max_schedule={:?}）",
            info.num_ports,
            info.sample_rate,
            info.render_block_us,
            info.max_schedule
        );
        Ok(())
    }

    fn on_notify(&self, notification: Notification) {
        match notification {
            Notification::EngineStopped => {
                self.flags.fetch_or(FLAG_ENGINE_STOPPED, Ordering::Release);
                *self.notice.lock() =
                    Some("Maestro 引擎已停止（设置变更或重启失败），正在尝试恢复".to_string());
                tracing::warn!("Maestro: 引擎停止（设置变更或重启失败）");
            }
            Notification::EngineDisabled => {
                self.flags.fetch_or(FLAG_ENGINE_DISABLED, Ordering::Release);
                *self.notice.lock() =
                    Some("Maestro 已禁用本应用配置，请在 Maestro 设置中启用".to_string());
                tracing::warn!("Maestro: 应用配置被用户禁用");
            }
            Notification::SettingsChanged => {
                self.flags
                    .fetch_or(FLAG_SETTINGS_CHANGED, Ordering::Release);
                tracing::info!("Maestro: 用户设置已变更");
            }
            Notification::EngineStarted => {
                self.flags
                    .fetch_and(!FLAG_ENGINE_STOPPED, Ordering::Release);
                tracing::info!("Maestro: 引擎已启动");
            }
            _ => {}
        }
    }

    fn current_engine(&self) -> Option<Arc<Realtime>> {
        self.engine.read().as_ref().cloned()
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    fn max_port(&self) -> u8 {
        self.max_port.load(Ordering::Acquire)
    }

    fn recovery_needed(&self) -> bool {
        // Disabled 是用户选择，重开只会再次失败；交给调用方提示配置。
        self.flags.load(Ordering::Acquire) & FLAG_ENGINE_STOPPED != 0
    }

    fn info(&self) -> Option<Info> {
        *self.info.read()
    }
}

impl Maestro {
    /// 创建 Maestro 后端：加载库并按 `midi_max_port` 打开实时引擎。
    ///
    /// 该调用可能阻塞约 1 秒（Maestro 加载音色库/启动音频流），调用方必须放在
    /// 后台线程（`MidiManager` 的异步初始化路径）。
    pub fn new(app_id: &str, midi_max_port: u8) -> Result<Self, Error> {
        #[cfg(target_os = "android")]
        {
            let _ = (app_id, midi_max_port);
            return Err(Error::InitFailed(
                "Maestro 后端在 Android 上不可用，请改用 XSynth / LGS".to_string(),
            ));
        }

        #[cfg(not(target_os = "android"))]
        {
            let lib = MaestroLib::load().map_err(map_load_error)?;
            let version = format!("Maestro {}", lib.version());
            let shared = MaestroShared::new(lib, app_id, midi_max_port)?;
            Ok(Self { shared, version })
        }
    }

    /// 统计读取器（与后端共享引擎句柄，布局重建后自动读取新引擎）。
    pub fn stats_reader(&self) -> MaestroStatsReader {
        MaestroStatsReader {
            shared: Arc::clone(&self.shared),
        }
    }

    /// 实际生效的设置（端口数/采样率/调度窗口）。
    pub fn info(&self) -> Option<Info> {
        self.shared.info()
    }

    /// 打开 Maestro 针对本应用的设置窗口。
    pub fn show_settings(&self) -> Result<(), Error> {
        let realtime = self
            .shared
            .current_engine()
            .ok_or_else(|| Error::InitFailed("Maestro 引擎未运行".to_string()))?;
        realtime.show_settings().map_err(map_maestro_error)
    }
}

impl Drop for Maestro {
    /// API 句柄销毁时主动停掉引擎；输出连接即使还持有共享句柄，也不会让
    /// 旧 Maestro 音频流在后台继续运行。
    fn drop(&mut self) {
        *self.shared.engine.write() = None;
        self.shared.generation.fetch_add(1, Ordering::Release);
    }
}

impl Api for Maestro {
    fn version(&self) -> Option<String> {
        Some(self.version.clone())
    }

    fn inputs(&self) -> Result<Vec<InputInfo>, Error> {
        Ok(Vec::new())
    }

    fn outputs(&self) -> Result<Vec<OutputInfo>, Error> {
        Ok(vec![OutputInfo {
            id: 0,
            name: "Maestro".to_string(),
        }])
    }

    fn open_output(&self, id: u32) -> Result<Box<dyn PlaybackOutput>, Error> {
        if id != 0 {
            return Err(Error::DeviceNotFound(id));
        }
        Ok(Box::new(MaestroOutputConn::new(Arc::clone(&self.shared))))
    }

    fn open_input(
        &self,
        _id: u32,
        _callback: MidiInputCallback,
    ) -> Result<Box<dyn InputConnection>, Error> {
        Err(Error::InitFailed("Maestro 不支持 MIDI 输入".to_string()))
    }

    fn poll_stream_recovery_needed(&self) -> bool {
        self.shared.recovery_needed()
    }

    fn recover_stream(&mut self) -> Result<(), String> {
        let result = self.shared.install_engine(self.shared.max_port());
        if result.is_err() {
            // 失败后清标志，避免上层每帧重试一次 ~1s 的引擎启动。
            self.shared
                .flags
                .fetch_and(!FLAG_ENGINE_STOPPED, Ordering::Release);
        }
        result.map_err(|error| error.to_string())
    }
}

impl SynthControl for Maestro {
    /// 同布局：轻量 `reset`（清队列/控制器，不重开音频流）；异布局：重开引擎并热替换。
    fn set_midi_port_layout(&mut self, max_port: u8) -> Result<(), String> {
        let target = normalize_max_port(max_port);
        match layout_action(self.shared.max_port(), target) {
            LayoutAction::LightReset => {
                if let Some(realtime) = self.shared.current_engine() {
                    realtime
                        .reset()
                        .map_err(|error| format!("Maestro 轻量复位失败: {error}"))?;
                }
                Ok(())
            }
            LayoutAction::Rebuild => self
                .shared
                .install_engine(target)
                .map_err(|error| error.to_string()),
        }
    }
}

/// 音高弯音值 → 14bit（与默认 `OutputConnection::pitch_bend` 一致）。
pub(crate) fn pitch_bend_value(value: f32) -> (u8, u8) {
    let bend = ((value + 1.0) * 0.5 * f32::from(crate::constants::PITCH_BEND_MAX)).round() as u16;
    (
        (bend & u16::from(crate::constants::MIDI_VALUE_MASK)) as u8,
        ((bend >> 7) & u16::from(crate::constants::MIDI_VALUE_MASK)) as u8,
    )
}

/// 全局通道 → (port, channel)，供输出连接与单测共用。
pub(crate) fn channel_parts(ch: u16) -> (u8, u8) {
    let ch = ch.min(255);
    ((ch >> 4) as u8, (ch & 0x0F) as u8)
}

/// Maestro 的短消息入口（供单测替换/观察）。
#[cfg(test)]
pub(crate) fn short_message_for(ch: u16, status: u8, d1: u8, d2: u8) -> (u8, u32) {
    let (port, channel) = channel_parts(ch);
    (
        port,
        maestro_api_rs::short_message(status | channel, d1, d2),
    )
}

#[cfg(test)]
mod tests;
