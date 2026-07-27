//! Comet 渲染风格的 CPU 端逐帧渲染器（模块化结构）

mod helpers;
pub(crate) use helpers::collect_active_notes;
pub(crate) use helpers::collect_visible_notes;
pub(crate) use helpers::fill_bgra_black;
pub(crate) use helpers::fill_bgra_rect;
pub(crate) use helpers::hsv_to_rgb;
pub(crate) use helpers::is_black_key;
pub(crate) use helpers::note_color;

mod enhanced;
pub(crate) use enhanced::render_enhanced_frame;

mod miditrail;
pub(crate) use miditrail::render_miditrail_frame;

mod pfa;
pub(crate) use pfa::render_pfa_frame;

mod velocities;
pub(crate) use velocities::render_velocities_frame;

mod channels;
pub(crate) use channels::render_channels_frame;
