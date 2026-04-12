use std::time::Instant;
use std::hint::black_box;

#[test]
fn run_grid_benchmark() {
    let viewport_width = 3840.0;
    let viewport_height = 2160.0;
    let keyboard_width = 60.0;
    let ruler_height = 30.0;
    let zoom_x = 0.002; 
    let zoom_y = 2160.0 / 128.0; 
    let scroll_x = 0.0;
    let scroll_y = 0.0;
    let iterations = 10_000;

    let start = Instant::now();
    for _ in 0..iterations {
        let mut instances = Vec::with_capacity(5000);
        let visible_tick_start: f32 = scroll_x / zoom_x;
        let visible_tick_end: f32 = (scroll_x + viewport_width - keyboard_width) / zoom_x;
        let visible_key_start: f32 = scroll_y / zoom_y;
        let visible_key_end: f32 = (scroll_y + viewport_height - ruler_height) / zoom_y;

        let ticks_per_measure = 1920.0;
        let measure_start = (visible_tick_start / ticks_per_measure).floor() as i32;
        let measure_end = (visible_tick_end / ticks_per_measure).ceil() as i32;
        for _measure in measure_start..=measure_end {
            instances.push(black_box([0.0f32; 8]));
        }

        let ticks_per_beat = 480.0;
        let beat_start = (visible_tick_start / ticks_per_beat).floor() as i32;
        let beat_end = (visible_tick_end / ticks_per_beat).ceil() as i32;
        for beat in beat_start..=beat_end {
            if (beat as f32 * ticks_per_beat) % ticks_per_measure == 0.0 { continue; }
            instances.push(black_box([0.0f32; 8]));
        }

        let key_start = visible_key_start.floor() as i32;
        let key_end = visible_key_end.ceil() as i32;
        for _key in key_start..=key_end {
            instances.push(black_box([0.0f32; 8]));
        }
        black_box(instances);
    }
    let old_duration = start.elapsed();

    let start_new = Instant::now();
    for _ in 0..iterations {
        let camera_uniform = black_box([
            viewport_width, viewport_height, 
            scroll_x, scroll_y, 
            zoom_x, zoom_y, 
            keyboard_width, ruler_height
        ]);
        black_box(camera_uniform);
    }
    let new_duration = start_new.elapsed();

    println!("老架构总耗时: {:?}, 单帧: {:?}", old_duration, old_duration / iterations as u32);
    println!("新架构总耗时: {:?}, 单帧: {:?}", new_duration, new_duration / iterations as u32);
}
