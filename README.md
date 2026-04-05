### netmind — your browsing buddy

A Rust desktop tamagotchi companion that watches your browsing and comments
on what you're up to.  Runs a local LLM (Qwen3.5-0.8B) via llama.cpp with
automatic GPU acceleration — fully offline, no API keys needed.

Each user gets a unique procedurally-generated pixel creature based on their
machine identity.  The creature reacts to your network activity in real time:
dot-art limbs animate, expressions change, and it comments on what it sees.

#### Quick start

```bash
# macOS — Metal GPU auto-detected, no flags needed
cargo run --release

# Linux / Windows with NVIDIA GPU
cargo run --release --features cuda

# Any platform (CPU fallback)
cargo run --release
```

Model weights (~812 MB GGUF) download automatically on first launch and are
cached at `~/.cache/huggingface/hub`.

#### First-time setup

**1. Trust the CA cert** (generated at `certs/ca.crt` on first launch):

macOS:
```bash
sudo security add-trusted-cert -d -r trustRoot \
  -k /Library/Keychains/System.keychain certs/ca.crt
```

Linux (Debian/Ubuntu):
```bash
sudo cp certs/ca.crt /usr/local/share/ca-certificates/netmind.crt
sudo update-ca-certificates
```

Windows:
```powershell
certutil -addstore -f "ROOT" certs\ca.crt
```

**2. Set system proxy** (browsers use the system proxy):

| Setting | Value |
|---------|-------|
| HTTP Proxy | `127.0.0.1:8080` |
| HTTPS Proxy | `127.0.0.1:8080` |

On macOS: System Settings → Network → Wi-Fi → Details → Proxies.

**Remember to disable the proxy when netmind isn't running.**

#### How it works

```
Browser → MITM proxy (hudsucker) → sled DB → LLM insight → pixel creature speech
```

- **Proxy**: intercepts HTTP/S traffic and stores page content locally
- **LLM**: Qwen3.5-0.8B runs locally via llama.cpp (C++ engine with fused
  Metal/CUDA kernels for the hybrid gated delta net architecture)
- **Insights**: every 30 minutes (or on-demand via ASK button), recent browsing
  is summarized and the LLM generates a short casual observation
- **Creature**: DNA-driven procedural generation — body shape, ears, eyes, tail,
  patterns, limbs, and markings are all unique per machine

#### Architecture

```
src/
├── main.rs          # Boot: tokio backend thread + nannou GUI on main thread
├── proxy.rs         # MITM proxy (hudsucker) — intercepts browsing
├── store.rs         # sled DB — persists intercepted pages
├── insights.rs      # Prompt building + pattern detection
├── llm.rs           # LLM engine — llama.cpp GGUF inference
├── gui.rs           # nannou window, layout, rendering
├── creature.rs      # Procedural creature generation + dot-art rendering
├── bitmap_font.rs   # 3×5 pixel bitmap font for status bar text
├── state.rs         # Shared app state (proxy ↔ GUI bridge)
└── lib.rs           # Re-exports for examples/tests
```

Key design decisions:
- **nannou on main thread** — required by macOS/wgpu; tokio runs on a background thread
- **std::sync::mpsc** channels bridge tokio ↔ nannou (nannou model is `!Send`)
- **llama.cpp via `llama-cpp-2`** — Rust FFI bindings to the C++ inference engine;
  handles Metal/CUDA/CPU automatically with fused kernels for Qwen3.5's hybrid
  gated delta net + GQA architecture
- **GGUF Q8_0 quantization** — 812 MB model file (vs 1.7 GB unquantized), minimal
  quality loss, ~0.15s per inference on Metal

#### Cross-platform support

| Platform | GPU | Build command |
|----------|-----|---------------|
| macOS | Metal (auto) | `cargo build --release` |
| Linux | CUDA | `cargo build --release --features cuda` |
| Windows | CUDA | `cargo build --release --features cuda` |
| Any | CPU | `cargo build --release` |

Metal is automatically enabled on macOS at compile time — no feature flag needed.
The GUI uses nannou (wgpu-based) which supports macOS, Linux (X11/Wayland),
and Windows.  macOS gets additional transparent-window fixes via cocoa/objc.

#### Port conflicts

The proxy tries ports 8080, 8081, 8082, 9080, 9090 in order. The actual port
is shown in the GUI status bar.

#### Model details

**Qwen3.5-0.8B** — a hybrid transformer with:
- 24 layers: 18 gated delta net (linear attention) + 6 full GQA layers
- 1024 hidden size, 248K vocab, ~812 MB (Q8_0 GGUF)
- Thinking mode (`<think>...</think>`) disabled for speed — responses are direct
- Inference: ~0.15s on Apple Silicon (Metal), ~0.5s on CPU

Inference is powered by [llama.cpp](https://github.com/ggml-org/llama.cpp)
via the [`llama-cpp-2`](https://crates.io/crates/llama-cpp-2) Rust crate,
which provides fused GPU kernels optimized for the gated delta net architecture.

#### Cleanup

Remove the CA cert:
```bash
# macOS
sudo security remove-trusted-cert -d certs/ca.crt
# Linux
sudo rm /usr/local/share/ca-certificates/netmind.crt && sudo update-ca-certificates
```

Delete browsing data:
```bash
rm -rf netmind_data
```
