#!/usr/bin/env python3
"""
Lumino Release Build Script (Cross-Platform)

Builds release binaries for all supported platforms.
Automatically detects the host OS and builds appropriate targets.

Usage:
    python publish.py [all|setup|linux-amd64|linux-arm64|windows-amd64|windows-arm64|macos]

On Linux:
    - Can build all platforms (Linux native + cross-compile Windows)
    - Can install dependencies automatically with 'setup' command

On Windows:
    - Builds x86_64 and aarch64 Windows binaries

On macOS:
    - Builds native macOS binary
"""

import argparse
import os
import platform
import shutil
import subprocess
import sys
from pathlib import Path
from typing import List, Optional


# ANSI colors
class Colors:
    BLUE = "\033[0;34m"
    GREEN = "\033[0;32m"
    YELLOW = "\033[1;33m"
    RED = "\033[0;31m"
    NC = "\033[0m"


def log_info(msg: str) -> None:
    print(f"{Colors.BLUE}[INFO]{Colors.NC} {msg}")


def log_ok(msg: str) -> None:
    print(f"{Colors.GREEN}[OK]{Colors.NC} {msg}")


def log_warn(msg: str) -> None:
    print(f"{Colors.YELLOW}[WARN]{Colors.NC} {msg}")


def log_err(msg: str) -> None:
    print(f"{Colors.RED}[ERR]{Colors.NC} {msg}")


class BuildContext:
    def __init__(self) -> None:
        self.project_dir = Path(__file__).parent.resolve()
        self.publish_dir = self.project_dir / "publish"
        self.host_os = platform.system().lower()

    def run(self, cmd: List[str], cwd: Optional[Path] = None, env: Optional[dict] = None) -> None:
        """Run a shell command, raise on failure."""
        merged_env = os.environ.copy()
        if env:
            merged_env.update(env)

        log_info(f"Running: {' '.join(cmd)}")
        result = subprocess.run(cmd, cwd=cwd or self.project_dir, env=merged_env)
        if result.returncode != 0:
            raise RuntimeError(f"Command failed: {' '.join(cmd)}")

    def mkdir(self, path: Path) -> None:
        path.mkdir(parents=True, exist_ok=True)


class LinuxBuilder:
    """Builds all targets on Linux (including cross-compilation)."""

    def __init__(self, ctx: BuildContext) -> None:
        self.ctx = ctx

    def install_deps(self) -> None:
        """Install system dependencies."""
        log_info("Installing system dependencies...")

        deps = [
            "build-essential",
            "gcc-mingw-w64-x86-64",
            "g++-mingw-w64-x86-64",
            "gcc-aarch64-linux-gnu",
            "g++-aarch64-linux-gnu",
            "clang",
            "lld",
            "llvm",
            "rpm",
            "fakeroot",
            "wget",
            "squashfs-tools",
        ]

        self.ctx.run(["sudo", "apt-get", "update"])
        self.ctx.run(["sudo", "apt-get", "install", "-y"] + deps)

        # Add arm64 architecture
        result = subprocess.run(
            ["dpkg", "--print-foreign-architectures"],
            capture_output=True, text=True
        )
        if "arm64" not in result.stdout:
            log_info("Adding arm64 architecture...")
            self.ctx.run(["sudo", "dpkg", "--add-architecture", "arm64"])
            self.ctx.run(["sudo", "apt-get", "update"])

        # Add Ubuntu ports repository
        ports_source = Path("/etc/apt/sources.list.d/ports-arm64.sources")
        if not ports_source.exists():
            log_info("Adding Ubuntu ports repository...")
            ports_content = """Types: deb
URIs: http://ports.ubuntu.com/ubuntu-ports/
Suites: noble noble-updates noble-backports noble-security
Components: main restricted universe multiverse
Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg
Architectures: arm64
"""
            subprocess.run(
                ["sudo", "tee", str(ports_source)],
                input=ports_content.encode(),
                check=True,
            )
            self.ctx.run(["sudo", "apt-get", "update"])

        # Install arm64 dev libraries
        arm64_deps = [
            "libssl-dev:arm64",
            "libasound2-dev:arm64",
            "libvulkan-dev:arm64",
            "libx11-dev:arm64",
            "libwayland-dev:arm64",
            "libfontconfig-dev:arm64",
            "libfreetype6-dev:arm64",
            "libxkbcommon-dev:arm64",
            "libegl1-mesa-dev:arm64",
        ]
        self.ctx.run(["sudo", "apt-get", "install", "-y"] + arm64_deps)
        log_ok("System dependencies installed")

    def install_rust_tools(self) -> None:
        """Install Rust targets and cargo plugins."""
        log_info("Installing Rust targets...")

        for target in [
            "x86_64-pc-windows-gnu",
            "aarch64-unknown-linux-gnu",
            "aarch64-pc-windows-gnullvm",
        ]:
            subprocess.run(["rustup", "target", "add", target], check=False)

        # Install cargo plugins
        for plugin in ["cargo-deb", "cargo-generate-rpm"]:
            result = subprocess.run(["which", plugin], capture_output=True)
            if result.returncode != 0:
                log_info(f"Installing {plugin}...")
                self.ctx.run(["cargo", "install", plugin])

        log_ok("Rust toolchain ready")

    def install_llvm_mingw(self) -> None:
        """Install llvm-mingw for aarch64 Windows cross-compilation."""
        llvm_mingw_dir = Path("/opt/llvm-mingw")
        if (llvm_mingw_dir / "bin").exists():
            log_ok("llvm-mingw already installed")
            return

        log_info("Downloading llvm-mingw...")
        tmp_dir = Path("/tmp/llvm-mingw-download")
        tmp_dir.mkdir(exist_ok=True)

        archive = tmp_dir / "llvm-mingw.tar.xz"
        url = (
            "https://github.com/mstorsjo/llvm-mingw/releases/download/20240619/"
            "llvm-mingw-20240619-ucrt-ubuntu-20.04-x86_64.tar.xz"
        )

        self.ctx.run(["wget", "-q", "--show-progress", url, "-O", str(archive)])
        log_info("Extracting llvm-mingw...")
        self.ctx.run(["sudo", "tar", "xf", str(archive), "-C", "/opt"])

        extracted = Path("/opt/llvm-mingw-20240619-ucrt-ubuntu-20.04-x86_64")
        self.ctx.run(["sudo", "mv", str(extracted), str(llvm_mingw_dir)])

        # Create stub libraries
        lib_dir = llvm_mingw_dir / "aarch64-w64-mingw32/lib"
        for stub in ["libgcc.a", "libgcc_s.a", "libgcc_eh.a"]:
            subprocess.run(
                ["sudo", "ar", "rcs", str(lib_dir / stub)],
                check=False,
            )

        shutil.rmtree(tmp_dir, ignore_errors=True)
        log_ok("llvm-mingw installed")

    def setup_cargo_config(self) -> None:
        """Write .cargo/config.toml for cross-compilation."""
        config_path = self.ctx.project_dir / ".cargo/config.toml"
        config_path.parent.mkdir(exist_ok=True)

        config = """[build]
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
"""
        config_path.write_text(config)
        log_ok(".cargo/config.toml configured")

    def build_linux_amd64(self) -> None:
        """Build x86_64 Linux binary and packages."""
        log_info("Building x86_64 Linux (native)...")
        self.ctx.run(["cargo", "build", "--release"])

        out_dir = self.ctx.publish_dir / "linux-amd64"
        self.ctx.mkdir(out_dir)
        shutil.copy(
            self.ctx.project_dir / "target/release/lumino-rs",
            out_dir / "lumino-rs",
        )

        log_info("Building x86_64 deb package...")
        self.ctx.run(["cargo", "deb", "--no-build"])
        deb_files = list(
            (self.ctx.project_dir / "target/debian").glob("lumino-rs_*_amd64.deb")
        )
        for deb in deb_files:
            shutil.copy(deb, out_dir)

        log_info("Building x86_64 rpm package...")
        self.ctx.run(["cargo", "generate-rpm"])
        rpm_files = list(
            (self.ctx.project_dir / "target/generate-rpm").glob("lumino-rs-*.x86_64.rpm")
        )
        for rpm in rpm_files:
            shutil.copy(rpm, out_dir)

        log_ok("linux-amd64 built")

    def build_appimage_amd64(self) -> None:
        """Build x86_64 AppImage."""
        log_info("Building x86_64 AppImage...")

        appimagetool = Path("/tmp/appimagetool")
        if not appimagetool.exists():
            self.ctx.run([
                "wget", "-q",
                "https://github.com/AppImage/appimagetool/releases/download/continuous/"
                "appimagetool-x86_64.AppImage",
                "-O", str(appimagetool),
            ])
            appimagetool.chmod(0o755)

        appdir = Path("/tmp/AppDir-x86_64")
        if appdir.exists():
            shutil.rmtree(appdir)

        (appdir / "usr/bin").mkdir(parents=True)
        (appdir / "usr/share/applications").mkdir(parents=True)
        (appdir / "usr/share/icons/hicolor/scalable/apps").mkdir(parents=True)

        shutil.copy(
            self.ctx.project_dir / "target/release/lumino-rs",
            appdir / "usr/bin/lumino-rs",
        )

        # Use Lumino icon as app icon
        icon_src = self.ctx.project_dir / "resources/icons/brands/Lumino.svg"
        icon_dst = appdir / "usr/share/icons/hicolor/scalable/apps/lumino-rs.svg"
        shutil.copy(icon_src, icon_dst)
        shutil.copy(icon_dst, appdir / "lumino-rs.svg")

        desktop = """[Desktop Entry]
Name=Lumino
Comment=Lumino - A Fast Black MIDI Editor
Exec=lumino-rs
Icon=lumino-rs
Type=Application
Categories=AudioVideo;Audio;Midi;
Terminal=false
"""
        (appdir / "usr/share/applications/lumino-rs.desktop").write_text(desktop)
        (appdir / "lumino-rs.desktop").write_text(desktop)

        self.ctx.run([
            str(appimagetool), str(appdir),
            str(self.ctx.publish_dir / "linux-amd64/lumino-rs-x86_64.AppImage"),
        ])
        log_ok("x86_64 AppImage built")

    def build_linux_arm64(self) -> None:
        """Build aarch64 Linux binary and packages."""
        log_info("Building aarch64 Linux (cross-compile)...")

        env = {
            "PKG_CONFIG_ALLOW_CROSS": "1",
            "PKG_CONFIG_PATH": "/usr/lib/aarch64-linux-gnu/pkgconfig",
            "PKG_CONFIG_SYSROOT_DIR": "/usr/aarch64-linux-gnu",
        }
        self.ctx.run(
            ["cargo", "build", "--release", "--target", "aarch64-unknown-linux-gnu"],
            env=env,
        )

        out_dir = self.ctx.publish_dir / "linux-arm64"
        self.ctx.mkdir(out_dir)
        shutil.copy(
            self.ctx.project_dir / "target/aarch64-unknown-linux-gnu/release/lumino-rs",
            out_dir / "lumino-rs",
        )

        log_info("Building aarch64 deb package...")
        self.ctx.run(
            ["cargo", "deb", "--target", "aarch64-unknown-linux-gnu", "--no-build"]
        )
        deb_files = list(
            (self.ctx.project_dir / "target/debian").glob("lumino-rs_*_arm64.deb")
        )
        for deb in deb_files:
            shutil.copy(deb, out_dir)

        log_info("Building aarch64 rpm package...")
        self.ctx.run(
            ["cargo", "generate-rpm", "--target-dir", "target/aarch64-unknown-linux-gnu"]
        )
        rpm_dir = self.ctx.project_dir / "target/aarch64-unknown-linux-gnu/generate-rpm"
        rpm_files = list(rpm_dir.glob("lumino-rs-*.rpm"))
        for rpm in rpm_files:
            shutil.copy(rpm, out_dir / "lumino-rs-0.1.0-1.aarch64.rpm")

        log_ok("linux-arm64 built")

    def build_windows_amd64(self) -> None:
        """Build x86_64 Windows binary."""
        log_info("Building x86_64 Windows (cross-compile)...")
        self.ctx.run(["cargo", "build", "--release", "--target", "x86_64-pc-windows-gnu"])

        out_dir = self.ctx.publish_dir / "windows-amd64"
        self.ctx.mkdir(out_dir)
        shutil.copy(
            self.ctx.project_dir / "target/x86_64-pc-windows-gnu/release/lumino-rs.exe",
            out_dir / "lumino-rs.exe",
        )
        log_ok("windows-amd64 built")

    def build_windows_arm64(self) -> None:
        """Build aarch64 Windows binary."""
        log_info("Building aarch64 Windows (cross-compile)...")

        env = {"PATH": f"/opt/llvm-mingw/bin:{os.environ.get('PATH', '')}"}
        self.ctx.run(
            ["cargo", "build", "--release", "--target", "aarch64-pc-windows-gnullvm"],
            env=env,
        )

        out_dir = self.ctx.publish_dir / "windows-arm64"
        self.ctx.mkdir(out_dir)
        shutil.copy(
            self.ctx.project_dir
            / "target/aarch64-pc-windows-gnullvm/release/lumino-rs.exe",
            out_dir / "lumino-rs.exe",
        )
        log_ok("windows-arm64 built")


class WindowsBuilder:
    """Builds Windows targets on Windows."""

    def __init__(self, ctx: BuildContext) -> None:
        self.ctx = ctx

    def build(self) -> None:
        log_info("Building Windows targets...")

        # x86_64
        log_info("Building x86_64 Windows...")
        self.ctx.run(["cargo", "build", "--release"])

        out_dir = self.ctx.publish_dir / "windows-amd64"
        self.ctx.mkdir(out_dir)
        shutil.copy(
            self.ctx.project_dir / "target/release/lumino-rs.exe",
            out_dir / "lumino-rs.exe",
        )
        log_ok("windows-amd64 built")

        # aarch64 (best effort)
        log_info("Building aarch64 Windows (best effort)...")
        result = subprocess.run(
            ["cargo", "build", "--release", "--target", "aarch64-pc-windows-msvc"]
        )
        if result.returncode == 0:
            out_dir = self.ctx.publish_dir / "windows-arm64"
            self.ctx.mkdir(out_dir)
            shutil.copy(
                self.ctx.project_dir / "target/aarch64-pc-windows-msvc/release/lumino-rs.exe",
                out_dir / "lumino-rs.exe",
            )
            log_ok("windows-arm64 built")
        else:
            log_warn("aarch64 Windows build skipped (toolchain not installed)")


class MacOSBuilder:
    """Builds macOS target."""

    def __init__(self, ctx: BuildContext) -> None:
        self.ctx = ctx

    def build(self) -> None:
        log_info("Building macOS target...")
        self.ctx.run(["cargo", "build", "--release"])

        out_dir = self.ctx.publish_dir / "macos"
        self.ctx.mkdir(out_dir)
        shutil.copy(
            self.ctx.project_dir / "target/release/lumino-rs",
            out_dir / "lumino-rs",
        )
        log_ok("macOS binary built")


def print_summary(ctx: BuildContext) -> None:
    """Print build summary."""
    print()
    print(f"{Colors.GREEN}{'=' * 40}{Colors.NC}")
    print(f"{Colors.GREEN}  All builds completed successfully!{Colors.NC}")
    print(f"{Colors.GREEN}{'=' * 40}{Colors.NC}")
    print()
    print("Published artifacts:")

    for f in sorted(ctx.publish_dir.rglob("*")):
        if f.is_file():
            size = f.stat().st_size
            if size > 1024 * 1024:
                size_str = f"{size / (1024 * 1024):.1f} MB"
            elif size > 1024:
                size_str = f"{size / 1024:.1f} KB"
            else:
                size_str = f"{size} B"
            print(f"  {f.name} ({size_str})")

    print()
    print(f"{Colors.BLUE}Output directory:{Colors.NC} {ctx.publish_dir}")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Lumino Release Build Script",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Commands:
  all           Build all supported targets for current platform (default)
  setup         Install all dependencies and configure environment
  linux-amd64   Build x86_64 Linux + deb/rpm/AppImage
  linux-arm64   Build aarch64 Linux + deb/rpm
  windows-amd64 Build x86_64 Windows
  windows-arm64 Build aarch64 Windows
  macos         Build macOS binary
""",
    )
    parser.add_argument(
        "command",
        nargs="?",
        default="all",
        choices=["all", "setup", "linux-amd64", "linux-arm64",
                 "windows-amd64", "windows-arm64", "macos"],
        help="Build command (default: all)",
    )
    args = parser.parse_args()

    ctx = BuildContext()
    ctx.mkdir(ctx.publish_dir)

    host = ctx.host_os

    if host == "linux":
        builder = LinuxBuilder(ctx)

        if args.command == "setup":
            builder.install_deps()
            builder.install_rust_tools()
            builder.install_llvm_mingw()
            builder.setup_cargo_config()
            log_ok("Setup complete!")
            return 0

        elif args.command == "linux-amd64":
            builder.build_linux_amd64()
            builder.build_appimage_amd64()

        elif args.command == "linux-arm64":
            builder.build_linux_arm64()

        elif args.command == "windows-amd64":
            builder.build_windows_amd64()

        elif args.command == "windows-arm64":
            builder.build_windows_arm64()

        elif args.command == "all":
            builder.install_deps()
            builder.install_rust_tools()
            builder.install_llvm_mingw()
            builder.setup_cargo_config()
            builder.build_linux_amd64()
            builder.build_appimage_amd64()
            builder.build_linux_arm64()
            builder.build_windows_amd64()
            builder.build_windows_arm64()
            print_summary(ctx)

        else:
            log_err(f"Command '{args.command}' is not supported on Linux")
            return 1

    elif host == "windows":
        builder = WindowsBuilder(ctx)

        if args.command == "all":
            builder.build()
            print_summary(ctx)
        elif args.command in ("windows-amd64", "windows-arm64"):
            log_err("Use 'all' on Windows to build both x86_64 and aarch64")
            return 1
        else:
            log_err(f"Command '{args.command}' is not supported on Windows")
            return 1

    elif host == "darwin":
        builder = MacOSBuilder(ctx)

        if args.command in ("all", "macos"):
            builder.build()
            print_summary(ctx)
        else:
            log_err(f"Command '{args.command}' is not supported on macOS")
            return 1

    else:
        log_err(f"Unsupported platform: {host}")
        return 1

    return 0


if __name__ == "__main__":
    sys.exit(main())
