# MediaFlow Proxy Light ⚡️ 

A high-performance streaming proxy service written in Rust, focusing on delivering efficient and reliable media content delivery with support for various transport protocols.

This is a lightweight Rust implementation of [MediaFlow Proxy](https://github.com/mhdzumair/mediaflow-proxy), optimized for performance and focusing on core streaming functionality.

## Features

### Stream Processing
- Proxy and forward HTTP/HTTPS streams efficiently
- Real-time stream forwarding with minimal overhead
- Configurable buffer sizes for optimal performance

### Proxy & Routing
- Advanced proxy routing system with support for:
  - Domain-based routing rules
  - Protocol-specific routing (HTTP/HTTPS)
  - Subdomain and wildcard patterns
  - Customizable SSL verification per route
- Support for HTTP/HTTPS/SOCKS4/SOCKS5 proxy forwarding
- Support for expired or self-signed SSL certificates
- Public IP address retrieval for Debrid services integration

### Security
- API password protection
- Parameter encryption support
- URL expiration support
- IP-based access control

## Installation

Download the latest release for your platform from the [Releases](https://gitlab.com/mhdzumair/mediaflow-proxy-light/-/releases) page:

- Linux: `mediaflow-proxy-light-linux-x86_64.tar.gz`
- Windows: `mediaflow-proxy-light-windows-x86_64.zip`
- macOS: `mediaflow-proxy-light-macos-x86_64.tar.gz`

### Using Docker

```bash
docker run -d \
  -p 8888:8888 \
  -e APP__SERVER__HOST=0.0.0.0 \
  -e APP__SERVER__PORT=8888 \
  -e APP__AUTH__API_PASSWORD=your-secure-password \
  registry.gitlab.com/your-project/mediaflow-proxy-light:latest
```

## Configuration

Configuration can be provided via a TOML file or environment variables.

### TOML Configuration
See [config-example.toml](/config-example.toml) for a complete example.

### Environment Variables

Use double underscores (`__`) to separate nested configuration:

```bash
# Server configuration
APP__SERVER__HOST=0.0.0.0
APP__SERVER__PORT=8888
APP__SERVER__WORKERS=4

# Proxy configuration
APP__PROXY__CONNECT_TIMEOUT=30
APP__PROXY__BUFFER_SIZE=8192
APP__PROXY__FOLLOW_REDIRECTS=true
APP__PROXY__PROXY_URL="socks5://proxy:1080"
APP__PROXY__ALL_PROXY=true

# Auth configuration
APP__AUTH__API_PASSWORD="your-secure-password"

# Transport routes (JSON format)
TRANSPORT_ROUTES='{
  "all://*.streaming.com": {
    "proxy": true,
    "proxy_url": "socks5://streaming-proxy:1080",
    "verify_ssl": true
  }
}'
```


## API Endpoints

### Proxy Stream
- `GET /proxy/stream` - Stream content through proxy
- `HEAD /proxy/stream` - Check content headers

### URL Generation
- `POST /proxy/generate_url` - Generate proxy URL with authentication token

### Health Check
- `GET /health` - Service health check

## Example Usage

### Basic Stream Proxy

```bash
# Simple stream proxy
mpv "http://localhost:8888/proxy/stream?d=https://example.com/video.mp4&api_password=your_password"

# With custom headers
mpv "http://localhost:8888/proxy/stream?d=https://example.com/video.mp4&h_referer=https://example.com&h_origin=https://example.com&api_password=your_password"
```

### Using with Debrid Services

The `/proxy/ip` endpoint allows you to retrieve the public IP address of the MediaFlow Proxy server, which is useful when working with Debrid services.

```bash
# Get proxy server's public IP
curl "http://localhost:8888/proxy/ip"
```

## Development

### Prerequisites

- Rust 1.84 or higher
- For Windows builds: MinGW-w64
- For SSL support: OpenSSL development libraries

### Building from Source

```bash
# Clone the repository
git clone https://github.com/mhdzumair/MediaFlow-Proxy-Light
cd mediaflow-proxy-light

# Build the project
cargo build --release

# Run with config file
CONFIG_PATH=./config.toml ./target/release/mediaflow-proxy-light
```

### Testing

```bash
# Run tests
cargo test

# Run with logging
RUST_LOG=debug,actix_web=debug cargo test

# Format code
cargo fmt

# Run linter
cargo clippy
```

## License

[MIT License](LICENSE)

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
