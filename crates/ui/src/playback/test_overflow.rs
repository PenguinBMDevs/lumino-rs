fn main() {
    let segment_ticks = f32::MAX;
    let segment_microseconds = (segment_ticks as f64 / 480.0) * 500000.0;
    println!("val: {}", segment_microseconds as u64);
}
