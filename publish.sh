#!/usr/bin/env bash
# ============================================================
# Lumino Release Build Script
# Builds release binaries for all supported platforms:
#   - linux-amd64 (native + deb/rpm/AppImage)
#   - linux-arm64 (cross-compile + deb/rpm)
#   - windows-amd64 (cross-compile)
#   - windows-arm64 (cross-compile)
#
# Run on: Ubuntu 24.04 x86_64 (or compatible)
# ============================================================

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

PROJECT_DIR="$(cd "$(dirname "$0")" && pwd)"
PUBLISH_DIR="$PROJECT_DIR/publish"

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_ok()   { echo -e "${GREEN}[OK]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_err()  { echo -e "${RED}[ERR]${NC} $1"; }

# ============================================================
# 1. Check and install system dependencies
# ============================================================
install_deps() {
    log_info "Checking system dependencies..."

    if ! command -v sudo &>/dev/null; then
        log_err "sudo is required"
        exit 1
    fi

    log_info "Installing build tools..."
    sudo apt-get update
    sudo apt-get install -y \
        build-essential \
        gcc-mingw-w64-x86-64 \
        g++-mingw-w64-x86-64 \
        gcc-aarch64-linux-gnu \
        g++-aarch64-linux-gnu \
        clang \
        lld \
        llvm \
        rpm \
        fakeroot \
        wget \
        squashfs-tools

    # Add arm64 architecture for cross-compilation libraries
    if ! dpkg --print-foreign-architectures | grep -q arm64; then
        log_info "Adding arm64 architecture..."
        sudo dpkg --add-architecture arm64
        sudo apt-get update
    fi

    # Add Ubuntu ports repository for arm64 packages
    PORTS_SOURCE="/etc/apt/sources.list.d/ports-arm64.sources"
    if [ ! -f "$PORTS_SOURCE" ]; then
        log_info "Adding Ubuntu ports repository for arm64 packages..."
        sudo tee "$PORTS_SOURCE" > /dev/null <<'EOF'
Types: deb
URIs: http://ports.ubuntu.com/ubuntu-ports/
Suites: noble noble-updates noble-backports noble-security
Components: main restricted universe multiverse
Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg
Architectures: arm64
EOF
        sudo apt-get update
    fi

    log_info "Installing arm64 development libraries..."
    sudo apt-get install -y \
        libssl-dev:arm64 \
        libasound2-dev:arm64 \
        libvulkan-dev:arm64 \
        libx11-dev:arm64 \
        libwayland-dev:arm64 \
        libfontconfig-dev:arm64 \
        libfreetype6-dev:arm64 \
        libxkbcommon-dev:arm64 \
        libegl1-mesa-dev:arm64

    log_ok "System dependencies installed"
}

# ============================================================
# 2. Check and install Rust toolchain
# ============================================================
install_rust_tools() {
    log_info "Checking Rust toolchain..."

    if ! command -v rustup &>/dev/null; then
        log_err "rustup not found. Please install Rust first: https://rustup.rs/"
        exit 1
    fi

    log_info "Installing Rust targets..."
    rustup target add x86_64-pc-windows-gnu || true
    rustup target add aarch64-unknown-linux-gnu || true
    rustup target add aarch64-pc-windows-gnullvm || true

    log_info "Installing cargo plugins..."
    if ! command -v cargo-deb &>/dev/null; then
        cargo install cargo-deb
    fi
    if ! command -v cargo-generate-rpm &>/dev/null; then
        cargo install cargo-generate-rpm
    fi

    log_ok "Rust toolchain ready"
}

# 3. 无法编译winaarch64，本人直接跳过

# ============================================================
# 4. Ensure .cargo/config.toml has cross-compile settings
# ============================================================
setup_cargo_config() {
    CARGO_CONFIG="$PROJECT_DIR/.cargo/config.toml"
    mkdir -p "$(dirname "$CARGO_CONFIG")"

    cat > "$CARGO_CONFIG" <<'EOF'
[build]
rustflags = ["--cap-lints", "warn"]

[target.x86_64-pc-windows-gnu]
linker = "x86_64-w64-mingw32-gcc"

[target.aarch64-unknown-linux-gnu]
linker = "aarch64-linux-gnu-gcc"

[target.aarch64-pc-windows-gnullvm]
linker = "/opt/llvm-mingw/bin/clang"
rustflags = [
    "--cap-lints", "warn",
    "-C", "link-args=--target=aarch64-pc-windows-gnu -fuse-ld=lld",
]
EOF

    log_ok ".cargo/config.toml configured"
}

# ============================================================
# 5. Build: x86_64 Linux (native)
# ============================================================
build_linux_amd64() {
    log_info "Building x86_64 Linux (native)..."
    cd "$PROJECT_DIR"
    cargo build --release

    mkdir -p "$PUBLISH_DIR/linux-amd64"
    cp target/release/lumino-rs "$PUBLISH_DIR/linux-amd64/"

    log_info "Building x86_64 deb package..."
    cargo deb --no-build
    cp target/debian/lumino-rs_*_amd64.deb "$PUBLISH_DIR/linux-amd64/"

    log_info "Building x86_64 rpm package..."
    cargo generate-rpm
    cp target/generate-rpm/lumino-rs-*.x86_64.rpm "$PUBLISH_DIR/linux-amd64/"

    log_ok "linux-amd64 built"
}

# ============================================================
# 6. Build AppImage for x86_64 Linux
# ============================================================
build_appimage_amd64() {
    log_info "Building x86_64 AppImage..."
    cd "$PROJECT_DIR"

    # Download appimagetool if needed
    APPIMAGETOOL="/tmp/appimagetool"
    if [ ! -x "$APPIMAGETOOL" ]; then
        wget -q "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage" -O "$APPIMAGETOOL"
        chmod +x "$APPIMAGETOOL"
    fi

    # Create AppDir
    APPDIR="/tmp/AppDir-x86_64"
    rm -rf "$APPDIR"
    mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications" \
             "$APPDIR/usr/share/icons/hicolor/scalable/apps" \
             "$APPDIR/usr/share/metainfo"

    cp target/release/lumino-rs "$APPDIR/usr/bin/"

    # Use Lumino icon as app icon
    cp "$PROJECT_DIR/resources/icons/brands/Lumino.svg" \
       "$APPDIR/usr/share/icons/hicolor/scalable/apps/lumino-rs.svg"
    cp "$APPDIR/usr/share/icons/hicolor/scalable/apps/lumino-rs.svg" "$APPDIR/"

    cat > "$APPDIR/usr/share/applications/lumino-rs.desktop" <<'EOF'
[Desktop Entry]
Name=Lumino
Comment=Lumino - A Fast Black MIDI Editor
Exec=lumino-rs
Icon=lumino-rs
Type=Application
Categories=AudioVideo;Audio;Midi;
Terminal=false
EOF
    cp "$APPDIR/usr/share/applications/lumino-rs.desktop" "$APPDIR/"

    # Build AppImage
    cd /tmp
    "$APPIMAGETOOL" "$APPDIR" "$PUBLISH_DIR/linux-amd64/lumino-rs-x86_64.AppImage"

    log_ok "x86_64 AppImage built"
}

# ============================================================
# 7. Build: aarch64 Linux (cross-compile)
# ============================================================
build_linux_arm64() {
    log_info "Building aarch64 Linux (cross-compile)..."
    cd "$PROJECT_DIR"

    export PKG_CONFIG_ALLOW_CROSS=1
    export PKG_CONFIG_PATH=/usr/lib/aarch64-linux-gnu/pkgconfig
    export PKG_CONFIG_SYSROOT_DIR=/usr/aarch64-linux-gnu

    cargo build --release --target aarch64-unknown-linux-gnu

    mkdir -p "$PUBLISH_DIR/linux-arm64"
    cp target/aarch64-unknown-linux-gnu/release/lumino-rs "$PUBLISH_DIR/linux-arm64/"

    log_info "Building aarch64 deb package..."
    cargo deb --target aarch64-unknown-linux-gnu --no-build
    cp target/debian/lumino-rs_*_arm64.deb "$PUBLISH_DIR/linux-arm64/"

    log_info "Building aarch64 rpm package..."
    cargo generate-rpm --target-dir target/aarch64-unknown-linux-gnu
    cp target/aarch64-unknown-linux-gnu/generate-rpm/lumino-rs-*.rpm \
       "$PUBLISH_DIR/linux-arm64/lumino-rs-0.1.0-1.aarch64.rpm"

    log_ok "linux-arm64 built"
}

# ============================================================
# 8. Build: x86_64 Windows (cross-compile)
# ============================================================
build_windows_amd64() {
    log_info "Building x86_64 Windows (cross-compile)..."
    cd "$PROJECT_DIR"
    cargo build --release --target x86_64-pc-windows-gnu

    mkdir -p "$PUBLISH_DIR/windows-amd64"
    cp target/x86_64-pc-windows-gnu/release/lumino-rs.exe \
       "$PUBLISH_DIR/windows-amd64/"

    log_ok "windows-amd64 built"
}

# 9. 无法编译winaarch64，本人直接跳过

# ============================================================
# 10. Summary
# ============================================================
print_summary() {
    echo ""
    echo -e "${GREEN}========================================${NC}"
    echo -e "${GREEN}  All builds completed successfully!${NC}"
    echo -e "${GREEN}========================================${NC}"
    echo ""
    echo "Published artifacts:"
    find "$PUBLISH_DIR" -type f | sort | while read -r f; do
        size="$(du -h "$f" | cut -f1)"
        echo "  $(basename "$f") (${size})"
    done
    echo ""
    echo -e "${BLUE}Output directory:${NC} $PUBLISH_DIR"
}

# ============================================================
# Main
# ============================================================
main() {
    cd "$PROJECT_DIR"
    mkdir -p "$PUBLISH_DIR"

    case "${1:-all}" in
        deps)
            install_deps
            ;;
        rust)
            install_rust_tools
            ;;
        setup)
            install_deps
            install_rust_tools
            install_llvm_mingw
            setup_cargo_config
            ;;
        linux-amd64)
            build_linux_amd64
            build_appimage_amd64
            ;;
        linux-arm64)
            build_linux_arm64
            ;;
        windows-amd64)
            build_windows_amd64
            ;;
        windows-arm64)
            build_windows_arm64
            ;;
        all)
            install_deps
            install_rust_tools
            install_llvm_mingw
            setup_cargo_config
            build_linux_amd64
            build_appimage_amd64
            build_linux_arm64
            build_windows_amd64
            build_windows_arm64
            print_summary
            ;;
        *)
            echo "Usage: $0 [deps|rust|setup|linux-amd64|linux-arm64|windows-amd64|windows-arm64|all]"
            echo ""
            echo "  deps          - Install system dependencies only"
            echo "  rust          - Install Rust targets and cargo plugins only"
            echo "  setup         - Install all dependencies and configure environment"
            echo "  linux-amd64   - Build x86_64 Linux + deb/rpm/AppImage"
            echo "  linux-arm64   - Build aarch64 Linux + deb/rpm"
            echo "  windows-amd64 - Build x86_64 Windows"
            echo "  windows-arm64 - Build aarch64 Windows"
            echo "  all           - Full build for all platforms (default)"
            exit 1
            ;;
    esac
}

main "$@"
