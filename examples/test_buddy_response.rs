//! End-to-end test: load Qwen3.5-0.8B via LlmEngine, generate buddy responses.
//!
//! Run with:  cargo run --release --example test_buddy_response

use anyhow::Result;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("netmind=info")
        .init();

    eprintln!("[test] loading Qwen3.5-0.8B via LlmEngine...");
    let mut engine = netmind::llm::LlmEngine::load()?;
    eprintln!("[test] model loaded ({})\n", engine.device_name());

    let prompts = [
        "The user is browsing Rust documentation about async/await and tokio. React in one short sentence.",
        "The user has been scrolling twitter for 30 minutes looking at memes. React in one short sentence.",
        "The user is shopping on Amazon looking at mechanical keyboards. React in one short sentence.",
    ];

    for (i, prompt) in prompts.iter().enumerate() {
        eprintln!("[test] --- prompt {} ---", i + 1);
        eprintln!("[test] {}", prompt);
        let response = engine.infer(prompt, 40)?;
        eprintln!("[test] response: {:?}", response);
        if response.is_empty() {
            eprintln!("[test] WARNING: empty response!");
        }
        eprintln!();
    }

    eprintln!("[test] done.");
    Ok(())
}
