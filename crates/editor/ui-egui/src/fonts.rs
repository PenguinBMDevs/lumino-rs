//! 系统 CJK 字体注入。
//!
//! egui 0.37 不再默认携带完整 CJK 字体；这里用 `fontdb` 定位系统字体，
//! 以 `FontData` 注入到 proportional / monospace 的 fallback 链末尾。
//! 默认优先 Microsoft YaHei（与生产 `UiConfig::default()` 的
//! `program_font_name` 一致）。

use std::sync::{Arc, OnceLock};

use egui::{FontData, FontDefinitions, FontFamily};

/// 已解析的字体字节 + TTC face index。
struct FontFace {
    name: String,
    bytes: Vec<u8>,
    index: u32,
}

static FONT_FACE: OnceLock<Option<FontFace>> = OnceLock::new();

/// 候选字体族（按优先级）。
const CANDIDATES: &[&str] = &[
    "Microsoft YaHei",
    "Microsoft YaHei UI",
    "Noto Sans CJK SC",
    "Source Han Sans SC",
    "SimHei",
    "Arial Unicode MS",
];

/// 在给定 Context 上安装一次 CJK 字体。
pub fn install(ctx: &egui::Context) {
    let marker = egui::Id::unique("lumino-ui-egui-fonts-installed");
    if ctx.data(|data| data.get_temp::<bool>(marker).unwrap_or(false)) {
        return;
    }

    if let Some(definitions) = definitions() {
        ctx.set_fonts(definitions);
    }
    ctx.data_mut(|data| data.insert_temp(marker, true));
}

/// 构造带 CJK fallback 的 `FontDefinitions`。
pub fn definitions() -> Option<FontDefinitions> {
    let face = resolve_face()?;
    let mut definitions = FontDefinitions::default();
    let key = "lumino-cjk".to_owned();
    definitions.font_data.insert(
        key.clone(),
        FontData {
            font: Arc::new(face.bytes.clone()),
            index: face.index,
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
            .push(key.clone());
    }

    tracing::info!(
        "egui: 注入 CJK 字体 '{}'（{} bytes，face {}）",
        face.name,
        face.bytes.len(),
        face.index
    );
    Some(definitions)
}

/// 已解析字体族名（用于诊断 / 文档）。
pub fn resolved_name() -> Option<&'static str> {
    resolve_face().map(|face| face.name.as_str())
}

fn resolve_face() -> Option<&'static FontFace> {
    FONT_FACE
        .get_or_init(|| {
            let mut database = fontdb::Database::new();
            database.load_system_fonts();

            for name in CANDIDATES {
                let query = fontdb::Query {
                    families: &[fontdb::Family::Name(name)],
                    weight: fontdb::Weight::NORMAL,
                    stretch: fontdb::Stretch::Normal,
                    style: fontdb::Style::Normal,
                };
                let Some(id) = database.query(&query) else {
                    continue;
                };
                let Some(face) = database.with_face_data(id, |bytes, index| FontFace {
                    name: (*name).to_owned(),
                    bytes: bytes.to_vec(),
                    index,
                }) else {
                    continue;
                };
                return Some(face);
            }

            tracing::warn!(
                "egui: 未找到候选 CJK 字体 {:?}，中文可能显示为豆腐块",
                CANDIDATES
            );
            None
        })
        .as_ref()
}
