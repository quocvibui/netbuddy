### netbuddy — your browsing buddy

A desktop pixel creature that watches your browsing and comments on what you're
up to. Runs a local LLM (Qwen3.5-0.8B) fully offline — no API keys needed.

#### Prerequisites

- [Rust toolchain](https://rustup.rs/) (stable)
- macOS, Linux, or Windows
- NVIDIA GPU + CUDA toolkit (optional, Linux/Windows only)

#### Build & run

```bash
# macOS (Metal GPU auto-detected)
cargo run --release

# Linux/Windows with NVIDIA GPU
cargo run --release --features cuda

# CPU-only (any platform)
cargo run --release
```

The model (~530 MB) downloads automatically on first launch and is cached at
`~/.cache/huggingface/hub`.

#### Proxy setup

netbuddy intercepts browser traffic through a local HTTPS proxy. On first launch
it generates a CA certificate at `certs/ca.crt`.

**1. Trust the certificate:**

```bash
# macOS
sudo security add-trusted-cert -d -r trustRoot \
  -k /Library/Keychains/System.keychain certs/ca.crt

# Linux (Debian/Ubuntu)
sudo cp certs/ca.crt /usr/local/share/ca-certificates/netbuddy.crt
sudo update-ca-certificates

# Windows
certutil -addstore -f "ROOT" certs\ca.crt
```

**2. Set your system HTTP/HTTPS proxy to `127.0.0.1:8080`.**

On macOS: System Settings > Network > Wi-Fi > Details > Proxies.

Disable the proxy when netbuddy isn't running.

#### Configuration

A `netbuddy.toml` file is created on first launch. Defaults:

```toml
auto_response = true
auto_response_interval = 30
max_tokens = 40
temperature = 1.0
```

#### Controls

| Key | Action |
|-----|--------|
| ESC | Quit |
| A | Ask buddy (generate insight now) |
| F | Toggle fullscreen |

#### Cleanup

```bash
# Remove trusted cert (macOS)
sudo security remove-trusted-cert -d certs/ca.crt

# Delete browsing data
rm -rf netbuddy_data
```
