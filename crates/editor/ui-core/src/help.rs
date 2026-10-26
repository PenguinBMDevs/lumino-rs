//! 帮助系统：话题、搜索、首次提示状态（DRAW-038）
//!
//! 话题正文只存中文原文；渲染/搜索时按当前语言走 `lumino_core::tr_in`。
//! 工具按钮的 tooltip 与话题 tip 共用 [`tool_display_name`]，避免两处文案漂移。

mod topics;

use std::collections::HashSet;

pub use topics::{HelpSection, HelpTip, HelpTopic, TOPICS};

use lumino_core::i18n::tr_in;
use lumino_extras::i18n::{Language, main_translations};
use lumino_message::Tool;

impl HelpSection {
    /// 分区显示名
    pub fn title(self, lang: Language) -> &'static str {
        tr_in(
            match self {
                Self::Basics => "基础",
                Self::Editing => "编辑",
                Self::Drawing => "绘制",
                Self::Reference => "参考",
            },
            lang,
        )
    }
}

impl HelpTopic {
    /// 话题标题（当前语言）
    pub fn title_text(&self, lang: Language) -> &'static str {
        tr_in(self.title, lang)
    }

    /// 话题正文（当前语言）
    pub fn body_text(&self, lang: Language) -> &'static str {
        tr_in(self.body, lang)
    }

    /// 话题 tip（当前语言）
    ///
    /// 工具类话题直接复用工具按钮显示名；其余话题用登记的短句。
    pub fn tip_text(&self, lang: Language) -> &'static str {
        match self.tip {
            HelpTip::Tool(tool) => tool_display_name(tool, lang),
            HelpTip::Text(text) => tr_in(text, lang),
        }
    }
}

/// 工具按钮显示名（工具栏 tooltip 与帮助 tip 的唯一来源）。
pub fn tool_display_name(tool: Tool, lang: Language) -> &'static str {
    let t = main_translations(lang);
    match tool {
        Tool::Pointer => t.tool_pointer,
        Tool::PointerYSelect => t.tool_pointer_y_select,
        Tool::Pencil => t.tool_pencil,
        Tool::Eraser => t.tool_eraser,
        Tool::Curve => t.tool_curve,
        Tool::Brush => t.tool_brush,
        Tool::Shape => t.tool_shape,
        Tool::ShapeSelect => t.tool_mouse,
        Tool::Text => t.tool_text,
        // 以下工具当前没有独立工具栏按钮，仍给出可读名（帮助/诊断用）。
        Tool::Pen => tr_in("钢笔工具", lang),
        Tool::Razor => tr_in("切割工具", lang),
        Tool::DrawEraser => tr_in("绘制橡皮", lang),
    }
}

/// 工具按钮 tooltip（与工具显示名同源；话题 tip 对同一工具使用同一来源）。
///
/// `Pointer` / `PointerYSelect` 共用「框选与多选」话题，但 tooltip 必须各自显示
/// 自己的按钮名，因此这里直接返回 [`tool_display_name`]，不取话题的单个 tip。
pub fn tool_tip(tool: Tool, lang: Language) -> Option<&'static str> {
    topic_for_tool(tool).map(|_| tool_display_name(tool, lang))
}

/// 工具首次使用时对应的话题。
pub fn topic_for_tool(tool: Tool) -> Option<&'static HelpTopic> {
    let id = match tool {
        Tool::Pointer | Tool::PointerYSelect => "selection",
        Tool::Pencil | Tool::Eraser | Tool::DrawEraser => "notes",
        Tool::Curve => "curve",
        Tool::Brush => "brush",
        Tool::Shape | Tool::ShapeSelect => "shape",
        Tool::Text => "text",
        Tool::Pen | Tool::Razor => return None,
    };
    topic(id)
}

/// 按 id 查话题。
pub fn topic(id: &str) -> Option<&'static HelpTopic> {
    TOPICS.iter().find(|topic| topic.id == id)
}

/// 搜索：标题 + 搜索词 + 正文子串；空查询返回全部。
///
/// 查询与候选都转小写；中文按原样子串匹配。英文界面同时保留中文原文搜索，
/// 这样用户输入中文关键词也不会因为界面语言而搜不到。
pub fn search(query: &str, lang: Language) -> Vec<&'static HelpTopic> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return TOPICS.iter().collect();
    }
    TOPICS
        .iter()
        .filter(|topic| {
            let mut haystacks = vec![
                topic.title_text(lang).to_lowercase(),
                topic.body_text(lang).to_lowercase(),
                topic.title.to_lowercase(),
                topic.body.to_lowercase(),
            ];
            haystacks.extend(topic.words.iter().map(|word| word.to_lowercase()));
            haystacks.iter().any(|text| text.contains(&needle))
        })
        .collect()
}

/// Help 窗口的本地状态。
#[derive(Debug, Clone, Default)]
pub struct HelpDialogState {
    /// 搜索框内容
    pub query: String,
    /// 当前选中项在**过滤结果**中的下标
    pub selected: usize,
}

impl HelpDialogState {
    /// 选中项变化时收敛到有效范围。
    pub fn clamp_selection(&mut self, results: usize) {
        if results == 0 {
            self.selected = 0;
        } else if self.selected >= results {
            self.selected = results - 1;
        }
    }
}

/// 首次使用提示状态。
#[derive(Debug, Clone, Default)]
pub struct HelpTipsState {
    seen: HashSet<String>,
    active: Option<&'static str>,
}

impl HelpTipsState {
    /// 从持久化配置恢复已看过的 topic id。
    pub fn from_seen(seen: impl IntoIterator<Item = String>) -> Self {
        Self {
            seen: seen.into_iter().collect(),
            active: None,
        }
    }

    /// 该话题是否应弹出首次提示。
    pub fn should_show(&self, id: &str) -> bool {
        !self.seen.contains(id)
    }

    /// 标记已看过并设为当前提示；重复调用同一话题不会重置 active。
    pub fn show(&mut self, id: &'static str) -> bool {
        if !self.seen.insert(id.to_string()) {
            return false;
        }
        self.active = Some(id);
        true
    }

    /// 当前活动的提示话题。
    pub fn active_topic(&self) -> Option<&'static HelpTopic> {
        self.active.and_then(topic)
    }

    /// 关闭当前提示（不改变已读集合）。
    pub fn dismiss(&mut self) {
        self.active = None;
    }

    /// 全部重现：清空已读与当前提示。
    pub fn reset(&mut self) {
        self.seen.clear();
        self.active = None;
    }

    /// 已读 id 快照（测试/诊断用）。
    pub fn seen_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.seen.iter().cloned().collect();
        ids.sort();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 话题表是数据驱动 key，`tr!` 扫描器看不到；这里显式守住英文登记。
    #[test]
    fn test_every_topic_has_english_translation() {
        for topic in TOPICS {
            assert!(
                lumino_core::i18n::lookup_en(topic.title).is_some(),
                "{} 标题缺英文",
                topic.id
            );
            assert!(
                lumino_core::i18n::lookup_en(topic.body).is_some(),
                "{} 正文缺英文",
                topic.id
            );
            if let HelpTip::Text(text) = topic.tip {
                assert!(
                    lumino_core::i18n::lookup_en(text).is_some(),
                    "{} tip 缺英文",
                    topic.id
                );
            }
        }
        for section in HelpSection::ALL {
            let title = match section {
                HelpSection::Basics => "基础",
                HelpSection::Editing => "编辑",
                HelpSection::Drawing => "绘制",
                HelpSection::Reference => "参考",
            };
            assert!(lumino_core::i18n::lookup_en(title).is_some());
        }
    }

    #[test]
    fn test_topic_texts_non_empty_and_searchable() {
        for lang in [Language::ZhCn, Language::EnUs] {
            for topic in TOPICS {
                assert!(!topic.id.is_empty());
                assert!(
                    !topic.title_text(lang).trim().is_empty(),
                    "{} 标题为空",
                    topic.id
                );
                assert!(
                    !topic.body_text(lang).trim().is_empty(),
                    "{} 正文为空",
                    topic.id
                );
                assert!(
                    !topic.tip_text(lang).trim().is_empty(),
                    "{} tip 为空",
                    topic.id
                );
            }
        }
        assert!(
            search("快捷键", Language::ZhCn)
                .iter()
                .any(|t| t.id == "shortcuts")
        );
        assert!(
            search("shortcut", Language::EnUs)
                .iter()
                .any(|t| t.id == "shortcuts")
        );
        assert!(
            search("960*4", Language::ZhCn)
                .iter()
                .any(|t| t.id == "expr")
        );
        assert!(search("不存在的关键词", Language::ZhCn).is_empty());
    }

    #[test]
    fn test_tip_seen_restored_from_config() {
        let tips = HelpTipsState::from_seen(vec!["curve".to_string()]);
        assert!(!tips.should_show("curve"));
        assert!(tips.should_show("text"));
    }

    #[test]
    fn test_tip_seen_and_reset() {
        let mut tips = HelpTipsState::default();
        assert!(tips.should_show("curve"));
        assert!(tips.show("curve"));
        assert!(!tips.should_show("curve"));
        assert!(!tips.show("curve"), "已看过的话题不得重复弹");
        assert_eq!(tips.active_topic().map(|t| t.id), Some("curve"));
        tips.dismiss();
        assert!(tips.active_topic().is_none());
        tips.reset();
        assert!(tips.should_show("curve"));
    }

    #[test]
    fn test_tool_tip_matches_tool_display_name() {
        for lang in [Language::ZhCn, Language::EnUs] {
            for tool in [
                Tool::Pointer,
                Tool::PointerYSelect,
                Tool::Pencil,
                Tool::Eraser,
                Tool::Curve,
                Tool::Brush,
                Tool::Shape,
                Tool::ShapeSelect,
                Tool::Text,
            ] {
                let tip = tool_tip(tool, lang).expect("已接入工具应有帮助话题");
                assert_eq!(tip, tool_display_name(tool, lang));
            }
        }
    }
}
