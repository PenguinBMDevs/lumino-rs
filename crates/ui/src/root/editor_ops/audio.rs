//! 编辑器操作 - 音频相关

use crate::root::Root;

impl Root {
    /// 获取并清空待处理的音频动作
    pub fn take_audio_actions(&mut self) -> Vec<crate::message::AudioAction> {
        self.editor.take_audio_actions()
    }
}
