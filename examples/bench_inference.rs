//! Benchmark: time model load + inference.
//! Run: cargo run --release --example bench_inference

use anyhow::Result;
use std::time::Instant;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("netmind=info")
        .init();

    eprintln!("[bench] loading model...");
    let t0 = Instant::now();
    let mut engine = netmind::llm::LlmEngine::load()?;
    eprintln!("[bench] model loaded in {:.2}s ({})", t0.elapsed().as_secs_f32(), engine.device_name());

    let prompt = "User is reading about Rust. React briefly.";
    eprintln!("[bench] prompt: {:?}", prompt);

    let t1 = Instant::now();
    let result = engine.infer(prompt, 30)?;
    eprintln!("[bench] inference: {:.2}s", t1.elapsed().as_secs_f32());
    eprintln!("[bench] result: {:?}", result);

    // Second call to test warm performance
    let t2 = Instant::now();
    let result2 = engine.infer("User is shopping on Amazon. React.", 30)?;
    eprintln!("[bench] inference 2: {:.2}s", t2.elapsed().as_secs_f32());
    eprintln!("[bench] result 2: {:?}", result2);

    Ok(())
}
