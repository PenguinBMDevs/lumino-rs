use lumino_core::locale::{self, LanguagePreference};
use lumino_core::storage::config::*;
use lumino_core::types::Language;
use std::{fs, io, path::PathBuf};

/// 语言覆盖环境变量（UI-018）：`LUMINO_LANG=zh-CN` / `en-US` / `zh_CN.UTF-8`。
///
/// 用于无 GUI 排障、CI、以及系统语言与实际期望不符的罕见表述——优先级最高。
/// ⚠️ 它只决定**本次启动**的生效语言；若本次运行中用户改了任意设置导致配置落盘，
/// 生效语言会被一并写入（`language_preference` 落 `Fixed`），即环境变量不再需要。
const ENV_LANG: &str = "LUMINO_LANG";

#[derive(Debug)]
pub struct ConfigWrapper {
    inner: Config,
    path: PathBuf,
    dirty: bool,
    /// 是否为缺少 GPU 字段的旧版配置（加载时已归一化为"抑制警告"）
    legacy_gpu: bool,
    /// 界面语言的解析来源说明（UI-018）。
    ///
    /// 由调用方在**文件日志可用之后**打印——本模块加载配置时文件日志层还没接上
    /// （`logging::start_file_logging` 依赖刚加载出来的配置），在这里 `info!` 只会进
    /// stderr，用户回传的日志文件里看不到。
    language_source: String,
}

impl ConfigWrapper {
    pub fn new(path: PathBuf) -> io::Result<Self> {
        Self::new_with_language_override(path, std::env::var(ENV_LANG).ok())
    }

    /// [`ConfigWrapper::new`] 的可注入版本：语言覆盖由调用方给出。
    ///
    /// 唯一目的是让「加载 → 解析语言」这条链路可被**确定性**测试——单测直接传
    /// `None`，不必依赖（也不得篡改）进程环境变量。
    pub fn new_with_language_override(
        path: PathBuf,
        language_override: Option<String>,
    ) -> io::Result<Self> {
        let existed = path.exists() || path.with_extension("toml").exists();
        let mut inner = Self::load_or_migrate(&path)?;
        // 旧版配置（文件存在但缺 GPU 字段）：归一化为抑制警告，保证"老用户升级静默"
        let legacy_gpu = existed && inner.ui.gpu_warning_suppressed.is_none();
        if legacy_gpu {
            inner.ui.gpu_warning_suppressed = Some(true);
        }
        // UI-018：解析本次启动的界面语言——环境变量 > 显式固定 > 系统显示语言；
        // 老配置（有文件但没写过偏好）保留既有值，绝不静默覆盖用户选择。
        let (language, source) =
            resolve_language_with_env(&inner.ui, existed, language_override.as_deref());
        inner.ui.language = language;
        if !existed && inner.ui.language_preference.is_none() {
            // 全新安装：把「跟随系统」固化进配置，供设置面板回显与下次启动沿用
            inner.ui.language_preference = Some(LanguagePreference::System);
        }
        tracing::debug!("界面语言解析 = {language}（{source}）");
        Ok(Self {
            inner,
            path,
            dirty: false,
            legacy_gpu,
            language_source: source,
        })
    }

    /// 界面语言的解析来源说明（供启动日志打印，见字段文档）。
    pub fn language_source(&self) -> &str {
        &self.language_source
    }

    /// 加载配置文件，支持从旧版 TOML 格式迁移到 JSON
    fn load_or_migrate(path: &PathBuf) -> io::Result<Config> {
        // 优先读取 JSON 格式
        if path.exists() {
            let bytes = fs::read(path)?;
            match serde_json::from_slice(&bytes) {
                Ok(config) => return Ok(config),
                Err(e) => {
                    tracing::warn!("JSON 配置文件解析失败 ({}), 使用默认配置", e);
                    return Ok(Config::default());
                }
            }
        }

        // JSON 不存在时，尝试从旧版 TOML 迁移
        let toml_path = path.with_extension("toml");
        if toml_path.exists() {
            tracing::info!("检测到旧版 TOML 配置文件，迁移到 JSON 格式");
            let bytes = fs::read(&toml_path)?;
            match toml::from_slice::<Config>(&bytes) {
                Ok(config) => {
                    // 迁移成功，写入 JSON 格式
                    if let Some(parent) = path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    let json_str = serde_json::to_string_pretty(&config)
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                    fs::write(path, json_str.as_bytes())?;
                    // 删除旧版 TOML 文件
                    if let Err(e) = fs::remove_file(&toml_path) {
                        tracing::warn!("删除旧版 TOML 配置文件失败: {}", e);
                    }
                    tracing::info!("配置文件迁移完成: TOML -> JSON");
                    return Ok(config);
                }
                Err(e) => {
                    tracing::warn!("旧版 TOML 配置文件解析失败 ({}), 使用默认配置", e);
                    return Ok(Config::default());
                }
            }
        }

        // 都不存在，使用默认配置
        Ok(Config::default())
    }

    pub fn get(&self) -> &Config {
        &self.inner
    }
    /// 是否为旧版配置（缺 GPU 字段，已归一化为抑制警告）
    pub fn is_legacy_gpu_config(&self) -> bool {
        self.legacy_gpu
    }
    pub fn patch<F>(&mut self, f: F)
    where
        F: FnOnce(&mut Config),
    {
        f(&mut self.inner);
        self.dirty = true;
    }
    pub fn save(&mut self) -> io::Result<()> {
        if !self.dirty {
            return Ok(());
        }

        let str = serde_json::to_string_pretty(&self.inner)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&self.path, str.as_bytes())?;
        self.dirty = false;
        Ok(())
    }
}

/// UI-018：解析启动时的界面语言，返回（生效语言, 来源说明）。
///
/// 优先级：`LUMINO_LANG` 环境变量 > 配置里的显式固定 > 系统显示语言。
/// 老配置（有文件但没写过 `language_preference`）语义上等价于「用户固定」，
/// **不覆盖**——否则用户手动选过的英文会在下次启动被系统语言悄悄改回去。
///
/// 环境变量由参数注入（不在此函数内读进程环境），使单测可确定性覆盖全部分支。
fn resolve_language_with_env(
    ui: &UiConfig,
    existed: bool,
    env_override: Option<&str>,
) -> (Language, String) {
    if let Some(raw) = env_override {
        if let Some(lang) = Language::from_tag(raw) {
            return (lang, format!("来源：环境变量 {ENV_LANG}={raw}"));
        }
        tracing::warn!("{ENV_LANG}=\"{raw}\" 无法识别为语言标签，已忽略");
    }
    match ui.language_preference {
        Some(LanguagePreference::Fixed(lang)) => (lang, "来源：用户在设置中固定".to_string()),
        Some(LanguagePreference::System) => detect_with_source(),
        // 老配置：文件里没有 `language_preference`，语义上按「用户固定」处理，
        // 宁可不跟随系统，也不覆盖用户可能已经选过的语言。
        None if existed => (ui.language, "来源：老配置保留（未声明偏好）".to_string()),
        None => detect_with_source(),
    }
}

/// 探测系统显示语言并附来源说明（含实际命中的 tag，便于排障）。
fn detect_with_source() -> (Language, String) {
    let (lang, tag) = locale::detect_system_language_with_tag();
    match tag {
        Some(tag) => (lang, format!("来源：系统显示语言 {tag}")),
        None => (lang, "来源：系统语言无法判定，回落默认".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ui_with(preference: Option<LanguagePreference>, language: Language) -> UiConfig {
        UiConfig {
            language,
            language_preference: preference,
            ..UiConfig::default()
        }
    }

    #[test]
    fn test_resolve_language_fixed_is_respected() {
        let ui = ui_with(
            Some(LanguagePreference::Fixed(Language::EnUs)),
            Language::EnUs,
        );
        let (lang, source) = resolve_language_with_env(&ui, true, None);
        assert_eq!(lang, Language::EnUs);
        assert!(source.contains("固定"), "来源应说明用户固定：{source}");
    }

    #[test]
    fn test_resolve_language_legacy_config_keeps_stored_language() {
        // 老配置（有文件、没写过偏好）：保留既有 language，不被系统检测覆盖。
        let ui = ui_with(None, Language::EnUs);
        let (lang, source) = resolve_language_with_env(&ui, true, None);
        assert_eq!(lang, Language::EnUs, "老配置的用户选择不能被系统检测覆盖");
        assert!(source.contains("老配置"), "来源应标注老配置保留：{source}");
    }

    #[test]
    fn test_resolve_language_fresh_install_follows_system() {
        // 全新安装走系统检测：具体语言取决于 CI 机器，只断言来源文案与取值合法。
        let ui = ui_with(None, Language::ZhCn);
        let (lang, source) = resolve_language_with_env(&ui, false, None);
        assert!(Language::all().contains(&lang));
        assert!(
            source.contains("系统显示语言") || source.contains("无法判定"),
            "来源应为系统检测：{source}"
        );
    }

    #[test]
    fn test_resolve_language_system_preference_detects() {
        // 用户显式选了「跟随系统」（配置里已有该字段）→ 走检测而非固定值。
        let ui = ui_with(Some(LanguagePreference::System), Language::ZhCn);
        let (lang, source) = resolve_language_with_env(&ui, true, None);
        assert!(Language::all().contains(&lang));
        assert!(
            source.contains("系统显示语言") || source.contains("无法判定"),
            "来源应为系统检测：{source}"
        );
    }

    #[test]
    fn test_resolve_language_env_override_has_top_priority() {
        let ui = ui_with(
            Some(LanguagePreference::Fixed(Language::ZhCn)),
            Language::ZhCn,
        );
        let (lang, source) = resolve_language_with_env(&ui, true, Some("en-US"));
        assert_eq!(lang, Language::EnUs);
        assert!(source.contains(ENV_LANG), "来源应标注环境变量：{source}");

        // POSIX 形态同样认
        let (lang, _) = resolve_language_with_env(&ui, true, Some("zh_CN.UTF-8"));
        assert_eq!(lang, Language::ZhCn);
    }

    #[test]
    fn test_resolve_language_unparsable_env_falls_back_to_config() {
        let ui = ui_with(
            Some(LanguagePreference::Fixed(Language::EnUs)),
            Language::EnUs,
        );
        // `LANG=C` 这类占位值判定不出语言 → 回落配置口径，而不是变成英文/中文的猜测
        let (lang, source) = resolve_language_with_env(&ui, true, Some("C"));
        assert_eq!(lang, Language::EnUs);
        assert!(source.contains("固定"), "{source}");
    }

    // ── 端到端：文件 → serde → 解析（不依赖进程环境变量，不碰用户真实配置）──

    /// 本测试专用的临时配置路径（带进程 id，避免与其它测试 / 其它进程互相干扰）。
    fn temp_config_path(tag: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("lumino-ui018-{tag}-{}.json", std::process::id()));
        let _ = fs::remove_file(&path);
        path
    }

    #[test]
    fn test_config_wrapper_fresh_install_follows_system() {
        // 无配置文件 = 全新安装 → 跟随系统，且偏好固化为 System 供设置面板回显
        let path = temp_config_path("fresh");
        let wrapper = ConfigWrapper::new_with_language_override(path.clone(), None)
            .expect("全新配置加载不应失败");
        let ui = &wrapper.get().ui;
        assert_eq!(ui.language, locale::detect_system_language());
        assert_eq!(ui.language_preference, Some(LanguagePreference::System));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_config_wrapper_legacy_json_keeps_user_language() {
        // 老 config.json：只有 language、没有偏好字段 → 必须保留用户语言，
        // 且**不**被强行写入偏好（升级不被静默改语言）
        let path = temp_config_path("legacy");
        fs::write(&path, br#"{"ui":{"language":"en-US"}}"#).expect("写测试配置不应失败");
        let wrapper = ConfigWrapper::new_with_language_override(path.clone(), None)
            .expect("老配置加载不应失败");
        let ui = &wrapper.get().ui;
        assert_eq!(ui.language, Language::EnUs, "老配置的 language 必须被保留");
        assert_eq!(ui.language_preference, None, "老配置不应被强行写入偏好");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_config_wrapper_explicit_system_preference_is_honored() {
        // 配置里显式写了「跟随系统」→ 即使 language 存的是 en-US 也走系统检测
        let path = temp_config_path("system");
        fs::write(
            &path,
            br#"{"ui":{"language":"en-US","language_preference":{"mode":"system"}}}"#,
        )
        .expect("写测试配置不应失败");
        let wrapper = ConfigWrapper::new_with_language_override(path.clone(), None)
            .expect("配置加载不应失败");
        let ui = &wrapper.get().ui;
        assert_eq!(ui.language, locale::detect_system_language());
        assert_eq!(ui.language_preference, Some(LanguagePreference::System));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_config_wrapper_roundtrip_persists_fixed_preference() {
        // 落盘 → 重新加载：「固定英文」的意图必须还在，并**覆盖**存储的 language
        // （`language` 是派生值，偏好才是用户意图——这正是两者分开存的意义）
        let path = temp_config_path("roundtrip");
        let mut wrapper = ConfigWrapper::new_with_language_override(path.clone(), None)
            .expect("配置加载不应失败");
        wrapper.patch(|c| {
            c.ui.language_preference = Some(LanguagePreference::Fixed(Language::EnUs));
        });
        wrapper.save().expect("保存不应失败");

        let reloaded = ConfigWrapper::new_with_language_override(path.clone(), None)
            .expect("重新加载不应失败");
        assert_eq!(reloaded.get().ui.language, Language::EnUs);
        assert_eq!(
            reloaded.get().ui.language_preference,
            Some(LanguagePreference::Fixed(Language::EnUs))
        );
        // 序列化形态固定（配置可读性 + 向后兼容的一道锁）
        let text = fs::read_to_string(&path).expect("读回配置不应失败");
        assert!(
            text.contains(r#""mode": "fixed""#) && text.contains(r#""language": "en-US""#),
            "偏好序列化形态漂移：{text}"
        );
        let _ = fs::remove_file(&path);
    }
}
