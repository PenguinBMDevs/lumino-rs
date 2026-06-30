//! AudioEngine 的状态管理 — 持有当前加载的 PreparedModel。

use crate::audio_model::PreparedModel;

/// 引擎状态 — 管理 PreparedModel 的原子替换。
pub(crate) struct EngineState {
    model: Option<PreparedModel>,
}

impl EngineState {
    pub(crate) fn new() -> Self {
        Self { model: None }
    }

    pub(crate) fn load_model(&mut self, model: PreparedModel) {
        self.model = Some(model);
    }

    pub(crate) fn model(&self) -> Option<&PreparedModel> {
        self.model.as_ref()
    }

    pub(crate) fn has_model(&self) -> bool {
        self.model.is_some()
    }
}
