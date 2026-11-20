//! 确定性 CJK 字体注入 + 程序字体（用户设置）安装。
//!
//! egui 0.37 不再默认携带完整 CJK 字体，而系统字体因平台而异：
//! Windows 有微软雅黑、macOS 有苹方、CI 的精简 Ubuntu 可能一个 CJK 字体都没有。
//! 这会让中文渲染逐像素不同，进而使跨平台视觉基线测试（bridge_parity /
//! bands_pixels）无法复现。
//!
//! 因此这里统一使用仓库内置的 Noto Sans SC（GB2312 子集），通过 `include_bytes!`
//! 编译期内嵌。所有平台、所有环境（开发机与 CI）使用同一份字体字节，
//! 中文渲染确定性一致——基线可在任意平台重新捕获并复现。
//!
//! 设置面板的「程序字体」（`program_font_name` / `program_font_path`）在 egui 侧
//! 的落点也是这里：解析结果由 `lumino_ui::font` 交给 [`configure`]，本模块把它
//! 插到比例字体链**最前面**；内置 CJK 仍留在链尾做字形回退。视觉基线测试不调用
//! [`configure`]，因此仍然只看到内置字体，跨平台确定性不受影响。

use std::sync::{Arc, OnceLock, RwLock};

use egui::{FontData, FontDefinitions, FontFamily};

/// 仓库内置 CJK 字体（GB2312 子集，约 2.2 MB），随二进制分发、无外部文件依赖。
///
/// 路径相对 `CARGO_MANIFEST_DIR`（`crates/editor/ui-egui`），上溯 3 级到工作区根。
const BUNDLED_CJK: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../resources/fonts/NotoSansSC-Regular.ttf"
));

/// 注入字体的家族键（proportional / monospace fallback 链末尾）。
const FONT_KEY: &str = "lumino-cjk";

/// 字体族名（诊断 / 文档用）。
const FONT_NAME: &str = "Noto Sans SC (bundled)";

/// 程序字体在 `FontDefinitions` 中的键（比例字体链首位）。
const PROGRAM_FONT_KEY: &str = "lumino-program-font";

/// 已解析的程序字体（系统字体名或自定义字体文件）。
#[derive(Debug, Clone)]
pub struct ProgramFont {
    /// 来源标识（系统字体名或文件路径），用于变更检测与日志。
    pub source: String,
    /// 家族名（诊断用）。
    pub family: String,
    /// 字体文件字节。
    pub bytes: Arc<Vec<u8>>,
    /// 字体集合（`.ttc`）内的字面索引。
    pub face_index: u32,
}

/// 全局字体策略：程序字体 + 代数（代数变化 = 已安装的 Context 需重建字体表）。
struct FontState {
    generation: u64,
    program: Option<ProgramFont>,
}

static FONT_STATE: OnceLock<RwLock<FontState>> = OnceLock::new();

fn state() -> &'static RwLock<FontState> {
    FONT_STATE.get_or_init(|| {
        RwLock::new(FontState {
            generation: 0,
            program: None,
        })
    })
}

fn read_state() -> std::sync::RwLockReadGuard<'static, FontState> {
    state()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn write_state() -> std::sync::RwLockWriteGuard<'static, FontState> {
    state()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 更新程序字体；返回是否发生变化。
///
/// 变化后所有已安装的 egui `Context` 会在下一帧调用 [`install`] 时重建字体表，
/// 因此设置面板改字体可以**立即生效**，无需重启。
pub fn configure(program: Option<ProgramFont>) -> bool {
    let mut state = write_state();
    let unchanged = match (&state.program, &program) {
        (None, None) => true,
        (Some(old), Some(new)) => old.source == new.source,
        _ => false,
    };
    if unchanged {
        return false;
    }
    match &program {
        Some(font) => tracing::info!(
            "程序字体已配置: {}（{}，{} bytes，face {}）",
            font.family,
            font.source,
            font.bytes.len(),
            font.face_index
        ),
        None => tracing::info!("程序字体未配置，使用内置默认字体"),
    }
    state.program = program;
    state.generation = state.generation.wrapping_add(1);
    true
}

/// 当前程序字体来源（诊断 / 测试）。
pub fn configured_source() -> Option<String> {
    read_state()
        .program
        .as_ref()
        .map(|font| font.source.clone())
}

/// 字体安装标记：记录 Context 已安装的代数。
///
/// `Id::unique` 在 egui 0.37 按固定种子哈希 source（不含调用位置），
/// 因此不同调用点 / 测试拿到的都是同一个 Id。
fn marker_id() -> egui::Id {
    egui::Id::unique("lumino-ui-egui-fonts-installed")
}

/// 在给定 Context 上安装字体，并确保缺字形时不会 panic。
///
/// `egui_kittest` 默认把 `MissingGlyphPolicy` 设为 `Panic`（用于捕捉测试里意外
/// 缺失的字形）。内置字体虽覆盖常用中文，但遇到生僻字仍可能缺字形；显式设为
/// `Tofu` 可让无头测试在任意环境都不因字形缺失而崩溃。
///
/// 生产环境 epaint 默认本就是 `Tofu`，故此改动不改变生产渲染行为。该调用幂等：
/// 同一代数下每帧重复调用无副作用（仅一次 `data` 查询）；程序字体变更后代数变化，
/// 下一帧自动重建字体表。
pub fn install(ctx: &egui::Context) {
    ctx.set_missing_glyph_policy(egui::MissingGlyphPolicy::Tofu);

    let marker = marker_id();
    // 先只读代数：已安装则直接返回，禁止逐帧构造 `FontDefinitions`
    // （内含 2.2 MB 内置字体字节，逐帧拷贝会直接吃掉帧时间）。
    let generation = read_state().generation;
    if ctx.data(|data| data.get_temp::<u64>(marker)) == Some(generation) {
        return;
    }

    // 构造期间代数可能又变了：以 `definitions` 实际使用的代数为准，
    // 保证下一帧一定会再重建一次，不会把旧字体误标成新代数。
    let (definitions, used_generation) = build_definitions();
    if let Some(definitions) = definitions {
        ctx.set_fonts(definitions);
    }
    ctx.data_mut(|data| data.insert_temp(marker, used_generation));
}

/// 构造带内置 CJK fallback 与程序字体的 `FontDefinitions`。
///
/// 内置字体编译期内嵌，恒可用，因此始终返回 `Some`（保留 `Option` 签名是为
/// 兼容既有调用方）。
pub fn definitions() -> Option<FontDefinitions> {
    build_definitions().0
}

/// 构造字体定义，并返回其对应的程序字体代数（供 [`install`] 判断是否需要重建）。
fn build_definitions() -> (Option<FontDefinitions>, u64) {
    let state = read_state();
    let generation = state.generation;
    let mut definitions = FontDefinitions::default();
    definitions.font_data.insert(
        FONT_KEY.to_owned(),
        FontData {
            font: Arc::new(BUNDLED_CJK.to_vec()),
            index: 0,
            tweak: Default::default(),
        }
        .into(),
    );

    // 默认拉丁字体在前，CJK 作为 fallback 追加在末尾。
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        definitions
            .families
            .entry(family)
            .or_default()
            .push(FONT_KEY.to_owned());
    }

    // 几何符号兜底：`Ubuntu-Light` 与内置 CJK 子集都不含 ← / → / ● / ▾
    // 这类符号，缺字形时会渲染成豆腐块。Hack 随 egui 编译期内嵌（跨平台
    // 一致），把它追加在比例字体链**末尾**：只接管前面所有字体都没有的
    // 字形，不改变任何现有拉丁 / 中文字符的渲染结果。
    definitions
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .push("Hack".to_owned());

    // 用户选择的程序字体：插在比例字体链最前面，缺字形仍逐级回退到内置 CJK。
    // 只作用于 Proportional——Monospace 的等宽语义（状态栏数字 / 导出预览）不能被
    // 比例字体破坏。
    if let Some(font) = &state.program {
        definitions.font_data.insert(
            PROGRAM_FONT_KEY.to_owned(),
            FontData {
                font: font.bytes.clone(),
                index: font.face_index,
                tweak: Default::default(),
            }
            .into(),
        );
        definitions
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, PROGRAM_FONT_KEY.to_owned());
    }

    tracing::debug!(
        "egui: 注入内置 CJK 字体 '{}'（{} bytes），程序字体 = {:?}，代数 {}",
        FONT_NAME,
        BUNDLED_CJK.len(),
        state.program.as_ref().map(|font| &font.family),
        generation
    );
    (Some(definitions), generation)
}

/// 已解析字体族名（用于诊断 / 文档）。
pub fn resolved_name() -> Option<&'static str> {
    Some(FONT_NAME)
}

#[cfg(test)]
mod tests;
