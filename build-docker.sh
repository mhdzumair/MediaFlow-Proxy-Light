#!/bin/bash

# Build script for MediaFlow Proxy Light Docker images
# This script demonstrates different approaches to building Docker images using pre-built binaries

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}MediaFlow Proxy Light - Docker Build Script${NC}"
echo -e "${GREEN}===========================================${NC}"

# Function to print usage
usage() {
    echo "Usage: $0 [OPTIONS]"
    echo "Options:"
    echo "  -t, --type TYPE     Build type: 'prebuilt', 'local', or 'source' (default: prebuilt)"
    echo "  -v, --version VER   Release version for prebuilt (e.g., v1.0.0)"
    echo "  -p, --platforms     Docker platforms (default: linux/amd64,linux/arm64)"
    echo "  --tag TAG          Docker tag (default: mediaflow-proxy-light)"
    echo "  --push             Push to registry"
    echo "  -h, --help         Show this help"
    exit 1
}

# Default values
BUILD_TYPE="prebuilt"
PLATFORMS="linux/amd64,linux/arm64"
TAG="mediaflow-proxy-light"
PUSH_FLAG=""
VERSION=""

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -t|--type)
            BUILD_TYPE="$2"
            shift 2
            ;;
        -v|--version)
            VERSION="$2"
            shift 2
            ;;
        -p|--platforms)
            PLATFORMS="$2"
            shift 2
            ;;
        --tag)
            TAG="$2"
            shift 2
            ;;
        --push)
            PUSH_FLAG="--push"
            shift
            ;;
        -h|--help)
            usage
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            usage
            ;;
    esac
done

case $BUILD_TYPE in
    "prebuilt")
        if [[ -z "$VERSION" ]]; then
            echo -e "${RED}Error: Version is required for prebuilt type${NC}"
            echo "Example: $0 -t prebuilt -v v1.0.0"
            exit 1
        fi

        echo -e "${YELLOW}Building using pre-built binaries from release $VERSION${NC}"
        echo -e "${YELLOW}Platforms: $PLATFORMS${NC}"
        echo -e "${YELLOW}Estimated build time: ~30 seconds${NC}"

        docker buildx build \
            -f Dockerfile.prebuilt \
            --platform "$PLATFORMS" \
            --build-arg RELEASE_VERSION="$VERSION" \
            -t "$TAG:$VERSION" \
            -t "$TAG:latest" \
            $PUSH_FLAG \
            .
        ;;

    "local")
        echo -e "${YELLOW}Building using locally compiled binaries${NC}"
        echo -e "${YELLOW}Note: You should compile binaries first with:${NC}"
        echo -e "${YELLOW}  cargo build --release --target x86_64-unknown-linux-gnu${NC}"
        echo -e "${YELLOW}  cargo build --release --target aarch64-unknown-linux-gnu${NC}"

        # Build for x86_64
        if [[ "$PLATFORMS" == *"linux/amd64"* ]]; then
            echo -e "${GREEN}Building for linux/amd64...${NC}"
            docker buildx build \
                -f Dockerfile.local \
                --platform linux/amd64 \
                --build-arg BINARY_PATH=target/x86_64-unknown-linux-gnu/release/mediaflow-proxy-light \
                -t "$TAG:latest-amd64" \
                $PUSH_FLAG \
                .
        fi

        # Build for arm64
        if [[ "$PLATFORMS" == *"linux/arm64"* ]]; then
            echo -e "${GREEN}Building for linux/arm64...${NC}"
            docker buildx build \
                -f Dockerfile.local \
                --platform linux/arm64 \
                --build-arg BINARY_PATH=target/aarch64-unknown-linux-gnu/release/mediaflow-proxy-light \
                -t "$TAG:latest-arm64" \
                $PUSH_FLAG \
                .
        fi
        ;;

    "source")
        echo -e "${YELLOW}Building from source (original method)${NC}"
        echo -e "${YELLOW}Platforms: $PLATFORMS${NC}"
        echo -e "${RED}Warning: This will take approximately 1 hour with QEMU!${NC}"

        docker buildx build \
            --platform "$PLATFORMS" \
            -t "$TAG:source" \
            $PUSH_FLAG \
            .
        ;;

    *)
        echo -e "${RED}Error: Invalid build type '$BUILD_TYPE'${NC}"
        echo "Valid types: prebuilt, local, source"
        exit 1
        ;;
esac

echo -e "${GREEN}Build completed successfully!${NC}"

# Show image information
if [[ -z "$PUSH_FLAG" ]]; then
    echo -e "${GREEN}Local images:${NC}"
    docker images | grep "$TAG" | head -5
fi