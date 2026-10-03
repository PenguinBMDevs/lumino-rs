use lumino_ui::editor::{note::Note, spatial_index::NoteSpatialIndex};
use std::time::Instant;

fn main() {
    println!("Generating notes...");
    let mut notes = Vec::new();
    let num_notes = 1_000_000;
    for i in 0..num_notes {
        notes.push(Note {
            tick: (i % 10000) as f32 * 10.0,
            key: (i % 128) as u16,
            length: 20.0,
            velocity: 100,
            channel: 0,
        });
    }

    println!("Building spatial index...");
    let start = Instant::now();
    let index = NoteSpatialIndex::from_notes(&notes);
    let build_time = start.elapsed();
    println!("Build tree took: {:?}", build_time);

    println!("Querying spatial index...");
    let mut result = Vec::with_capacity(1024);
    let start = Instant::now();
    // 模拟 10,000 次视口查询
    let queries = 10_000;
    for i in 0..queries {
        let tick_start = (i % 1000) as f32 * 50.0;
        let tick_end = tick_start + 1000.0;
        index.update_query(tick_start, tick_end, 40, 80, &mut result);
    }
    let query_time = start.elapsed();
    println!("{} queries took: {:?}", queries, query_time);
    println!("Average query time: {:?}", query_time / queries as u32);
    println!("Total results: {}", result.len());
}
