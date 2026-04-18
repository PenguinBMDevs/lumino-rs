//! 自定义精度对话框状态

use super::tuplet::{DotType, TupletType};

#[derive(Debug, Clone)]
pub struct CustomPrecisionDialog {
    pub is_open: bool,
    pub tuplet_count: String,
    pub tuplet_type: TupletType,
    pub dot_type: DotType,
    pub note_value: String,
    pub divisor: String,
}

impl Default for CustomPrecisionDialog {
    fn default() -> Self {
        Self {
            is_open: false,
            tuplet_count: "3".to_string(),
            tuplet_type: TupletType::Triplet,
            dot_type: DotType::None,
            note_value: "64".to_string(),
            divisor: "1".to_string(),
        }
    }
}

impl CustomPrecisionDialog {
    pub fn calculate_ticks(&self, ppq: u16) -> Option<f32> {
        let note_value = self.note_value.parse::<f32>().ok()?;
        let divisor = self.divisor.parse::<f32>().ok()?;

        if note_value == 0.0 || divisor == 0.0 {
            return None;
        }

        let base_ticks = (ppq as f32) * 4.0 / note_value;

        let tuplet_ratio = if self.dot_type != DotType::None {
            if let Ok(tuplet_count) = self.tuplet_count.parse::<f32>() {
                if tuplet_count > 1.0 {
                    (tuplet_count - 1.0) / tuplet_count
                } else {
                    1.0
                }
            } else {
                1.0
            }
        } else {
            1.0
        };

        let dot_multiplier = self.dot_type.multiplier();

        let final_ticks = base_ticks * tuplet_ratio * dot_multiplier / divisor;

        Some(final_ticks)
    }

    pub fn display_text(&self) -> String {
        let mut text = String::new();
        if self.tuplet_count != "1" && !self.tuplet_count.is_empty() {
            text.push_str(&self.tuplet_count);
            text.push(' ');
        }
        text.push_str(&self.note_value);
        text.push_str("分音符");
        if self.divisor != "1" && !self.divisor.is_empty() {
            text.push_str(" / ");
            text.push_str(&self.divisor);
        }
        text
    }
}
