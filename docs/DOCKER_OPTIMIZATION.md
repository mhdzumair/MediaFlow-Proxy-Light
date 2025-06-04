# Docker Build Optimization Guide

This guide explains how to dramatically reduce Docker build times for MediaFlow Proxy Light by using pre-built release artifacts instead of compiling from source.

## Problem Statement

The original Docker build process using QEMU for cross-compilation takes approximately **1 hour** to build for `linux/amd64` and `linux/arm64` platforms. In contrast:
- Native linux x86_64 build: **2m 29s**
- Native linux aarch64 build: **4m 49s**

## Solution: Use Pre-built Release Artifacts

Instead of compiling Rust code inside Docker, we download and use the pre-built binaries from GitHub releases. This reduces build time from **~1 hour to ~30 seconds**.

## Available Dockerfiles

### 1. `Dockerfile.prebuilt` (Recommended)
Downloads pre-built binaries from GitHub releases during Docker build.

**Pros:**
- Fastest build time (~30 seconds)
- Uses official release binaries
- Automatic platform detection
- No local compilation required

**Cons:**
- Requires internet access during build
- Only works with tagged releases

### 2. `Dockerfile.local`
Uses locally compiled binaries.

**Pros:**
- Works with unreleased code
- No internet required during Docker build
- Full control over compilation flags

**Cons:**
- Requires local cross-compilation setup
- Must build binaries before Docker build

### 3. `Dockerfile` (Original)
Compiles from source inside Docker.

**Pros:**
- Self-contained build process
- Works with any code state

**Cons:**
- Very slow (~1 hour with QEMU)
- Heavy resource usage

## Usage Examples

### Using Pre-built Binaries (Fastest)

```bash
# Build using release v1.0.0
./build-docker.sh -t prebuilt -v v1.0.0

# Build and push to registry
./build-docker.sh -t prebuilt -v v1.0.0 --push --tag myregistry/mediaflow-proxy-light

# Build for specific platform only
./build-docker.sh -t prebuilt -v v1.0.0 -p linux/amd64
```

### Using Local Binaries

```bash
# First, compile the binaries locally
cargo build --release --target x86_64-unknown-linux-gnu
cargo build --release --target aarch64-unknown-linux-gnu

# Then build Docker images
./build-docker.sh -t local
```

### Manual Docker Commands

```bash
# Using pre-built binaries
docker buildx build \
  -f Dockerfile.prebuilt \
  --platform linux/amd64,linux/arm64 \
  --build-arg RELEASE_VERSION=v1.0.0 \
  -t mediaflow-proxy-light:latest \
  .

# Using local binaries
docker buildx build \
  -f Dockerfile.local \
  --platform linux/amd64 \
  --build-arg BINARY_PATH=target/x86_64-unknown-linux-gnu/release/mediaflow-proxy-light \
  -t mediaflow-proxy-light:latest-amd64 \
  .
```

## GitHub Actions Integration

The release workflow has been updated to use the optimized Dockerfile:

```yaml
- name: Build and push (using pre-built binaries)
  uses: docker/build-push-action@v5
  with:
    context: .
    file: ./Dockerfile.prebuilt  # Uses optimized Dockerfile
    platforms: linux/amd64,linux/arm64
    build-args: |
      RELEASE_VERSION=${{ steps.version.outputs.RELEASE_VERSION }}
```

Key changes:
- Added `needs: build-binaries` to ensure binaries are available first
- Uses `Dockerfile.prebuilt` instead of the original Dockerfile
- Passes the release version as a build argument

## Performance Comparison

| Method | Build Time | Resource Usage | Use Case |
|--------|------------|----------------|----------|
| Pre-built | ~30 seconds | Low | Production releases |
| Local | ~5 minutes | Medium | Development with custom changes |
| Source | ~1 hour | High | Legacy/fallback method |

## Security Considerations

- **Pre-built method**: Downloads binaries from GitHub releases (secure, signed)
- **Local method**: Uses your own compiled binaries (maximum security)
- **Source method**: Compiles from source (traditional approach)

All methods use the same minimal `distroless/cc-debian11` base image for the runtime stage.

## Troubleshooting

### Build fails with "Unsupported platform"
The `Dockerfile.prebuilt` only supports `linux/amd64` and `linux/arm64`. Add support for other platforms by updating the case statement.

### Binary download fails
- Check if the release version exists on GitHub
- Verify internet connectivity during build
- Ensure the release has the expected binary artifacts

### Local binary not found
- Ensure you've compiled for the correct target architecture
- Check the `BINARY_PATH` build argument
- Verify the binary exists and is executable

## Migration Guide

To migrate from the original Dockerfile to the optimized version:

1. **For CI/CD**: Update your build scripts to use `-f Dockerfile.prebuilt`
2. **For releases**: The GitHub Actions workflow is already updated
3. **For development**: Use the `build-docker.sh` script for convenience

The original `Dockerfile` is preserved for backward compatibility.