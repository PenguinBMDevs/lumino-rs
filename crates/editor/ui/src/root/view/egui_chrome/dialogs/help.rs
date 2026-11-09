//! Help 窗口：生产状态 → egui 模型，以及动作 → `Message` 映射。

use lumino_core::tr;
use lumino_extras::i18n::main_translations;
use lumino_ui_core::help::{self, HelpSection};
use lumino_ui_egui::chrome::{HelpModel, HelpSectionModel, HelpTopicBodyModel, HelpTopicRowModel};

use crate::Message;

impl crate::root::Root {
    /// Help 模型：搜索框 / 分区话题列表 / 当前正文 / 底部按钮。
    pub(super) fn help_model(&self) -> HelpModel {
        let language = self.settings.display.language;
        let t = main_translations(language);
        let state = &self.state.help_dialog;
        let results = help::search(&state.query, language);
        let selected = results
            .get(state.selected)
            .copied()
            .or_else(|| results.first().copied());

        let sections = HelpSection::ALL
            .iter()
            .filter_map(|section| {
                let topics: Vec<HelpTopicRowModel> = results
                    .iter()
                    .enumerate()
                    .filter(|(_, topic)| topic.section == *section)
                    .map(|(index, topic)| HelpTopicRowModel {
                        index,
                        label: topic.title_text(language).to_owned(),
                        active: selected.is_some_and(|s| s.id == topic.id),
                    })
                    .collect();
                (!topics.is_empty()).then(|| HelpSectionModel {
                    title: section.title(language).to_owned(),
                    topics,
                })
            })
            .collect();

        let topic = selected.map(|topic| HelpTopicBodyModel {
            title: topic.title_text(language).to_owned(),
            tip: topic.tip_text(language).to_owned(),
            paragraphs: topic
                .body_text(language)
                .split("\n\n")
                .map(str::to_owned)
                .collect(),
        });

        HelpModel {
            titlebar: !self.settings.synth.use_native_titlebar,
            title: tr!("帮助").to_string(),
            search_hint: tr!("搜索帮助...").to_string(),
            query: state.query.clone(),
            sections,
            empty: tr!("没有匹配的话题").to_string(),
            topic,
            reset: tr!("全部重现").to_string(),
            close: t.file_close.to_owned(),
        }
    }

    /// egui Help 动作 → 生产 `Message`。
    pub(crate) fn help_action_message(
        &self,
        action: &lumino_ui_egui::help::HelpAction,
    ) -> Option<Message> {
        use lumino_message::HelpAction as Prod;
        use lumino_ui_egui::help::HelpAction as Action;
        Some(Message::Help(match action {
            Action::QueryChanged(query) => Prod::QueryChanged(query.clone()),
            Action::TopicSelected(index) => Prod::TopicSelected(*index),
            Action::ResetTips => Prod::ResetTips,
            Action::Close => Prod::Close,
        }))
    }
}
