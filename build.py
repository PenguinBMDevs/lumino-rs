#!/usr/bin/env python3
"""
Lumino Build Script (Cross-Platform)

Simple build script that compiles the project and copies the binary to bin/.

Usage:
    python build.py [release|debug]

Examples:
    python build.py           # Build in release mode (default)
    python build.py release   # Build in release mode
    python build.py debug     # Build in debug mode
"""

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path


def log_info(msg: str) -> None:
    print(f"[INFO] {msg}")


def log_ok(msg: str) -> None:
    print(f"[OK] {msg}")


def log_err(msg: str) -> None:
    print(f"[ERR] {msg}", file=sys.stderr)


def build(mode: str) -> int:
    """Build the project in the specified mode."""
    project_dir = Path(__file__).parent.resolve()

    if mode == "release":
        log_info("Building in release mode...")
        cmd = ["cargo", "build", "--release"]
        src_name = "lumino-rs.exe" if os.name == "nt" else "lumino-rs"
        src_dir = project_dir / "target/release"
    elif mode == "debug":
        log_info("Building in debug mode...")
        cmd = ["cargo", "build"]
        src_name = "lumino-rs.exe" if os.name == "nt" else "lumino-rs"
        src_dir = project_dir / "target/debug"
    else:
        log_err(f"Invalid mode: {mode}. Use 'release' or 'debug'.")
        return 1

    # Run cargo build
    result = subprocess.run(cmd, cwd=project_dir)
    if result.returncode != 0:
        log_err("Build failed!")
        return 1

    # Copy binary to bin/
    bin_dir = project_dir / "bin"
    bin_dir.mkdir(exist_ok=True)

    src = src_dir / src_name
    dst = bin_dir / src_name

    if not src.exists():
        log_err(f"Binary not found: {src}")
        return 1

    shutil.copy2(src, dst)
    log_ok(f"Build completed successfully!")
    log_info(f"Executable copied to: {dst}")

    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Lumino Build Script",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  python build.py           # Build in release mode (default)
  python build.py release   # Build in release mode
  python build.py debug     # Build in debug mode
""",
    )
    parser.add_argument(
        "mode",
        nargs="?",
        default="release",
        choices=["release", "debug"],
        help="Build mode (default: release)",
    )
    args = parser.parse_args()

    return build(args.mode)


if __name__ == "__main__":
    sys.exit(main())
