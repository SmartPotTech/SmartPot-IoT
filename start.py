"""
Startup and preparation script for simulating SmartPot-IoT-Rust in Wokwi.

Functions:
1. Checks and builds the ELF executable binary for ESP32 if it does not exist.
2. Verifies the Wokwi configuration (wokwi.toml and diagram.json).
3. Launches the local simulation via wokwi-cli if installed, or prints
   instructions to run it via the PyCharm / VS Code plugin.
"""

import shutil
import subprocess
import sys
from pathlib import Path

BASE_DIR = Path(__file__).resolve().parent
TARGET_ELF = BASE_DIR / "target" / "xtensa-esp32-none-elf" / "debug" / "smartpot-iot"
WOKWI_TOML = BASE_DIR / "wokwi.toml"
DIAGRAM_JSON = BASE_DIR / "diagram.json"


def ensure_binary_built():
    """Checks whether the ELF binary exists; if not, builds it using the esp toolchain."""
    if TARGET_ELF.exists():
        print(f"[OK] Firmware binary found: {TARGET_ELF}")
        return True

    print("[INFO] Binary not found. Building with cargo +esp...")
    try:
        cmd = ["cargo", "+esp", "build", "--bin", "smartpot-iot", "--ignore-rust-version"]
        subprocess.run(cmd, cwd=BASE_DIR, check=True)
        print(f"[OK] Build succeeded: {TARGET_ELF}")
        return True
    except (subprocess.CalledProcessError, FileNotFoundError) as e:
        print(f"[ERROR] Could not build firmware with cargo +esp: {e}")
        return False


def run_wokwi_simulation():
    """Runs the local simulation in Wokwi CLI or prints IDE instructions."""
    if not WOKWI_TOML.exists() or not DIAGRAM_JSON.exists():
        sys.exit("[ERROR] Missing required files: wokwi.toml or diagram.json")

    wokwi_cli = shutil.which("wokwi-cli")
    if wokwi_cli:
        print("[INFO] Starting local simulation with wokwi-cli...")
        try:
            subprocess.run([wokwi_cli, "."], cwd=BASE_DIR, check=True)
            return
        except subprocess.CalledProcessError as err:
            sys.exit(f"[ERROR] wokwi-cli exited with error: {err}")

    print("\n" + "=" * 65)
    print(" SmartPot-IoT-Rust · Wokwi Simulation Ready")
    print("=" * 65)
    print("Simulation files ready:")
    print(f"  * diagram.json: {DIAGRAM_JSON}")
    print(f"  * wokwi.toml:   {WOKWI_TOML}")
    print(f"  * ELF binary:   {TARGET_ELF}")
    print("\nOptions to start the simulation:")
    print("  1. In PyCharm / CLion (JetBrains):")
    print("     - Install the 'Wokwi Simulator' plugin.")
    print("     - Open 'diagram.json' and click the green 'Play' button (Start Simulation).")
    print("\n  2. In VS Code:")
    print("     - Install the 'Wokwi Simulator' extension.")
    print("     - Press F1 -> 'Wokwi: Start Simulator'.")
    print("\n  3. Via Wokwi CLI:")
    print("     - Install: npm install -g @wokwi/cli  (or cargo install wokwi-cli)")
    print("     - Run: wokwi-cli .")
    print("=" * 65 + "\n")


if __name__ == "__main__":
    if ensure_binary_built():
        run_wokwi_simulation()
