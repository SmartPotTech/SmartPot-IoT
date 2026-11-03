"""
Script de inicio y preparación para la simulación de SmartPot-IoT-Rust en Wokwi.

Funciones:
1. Verifica y construye el binario ejecutable ELF para ESP32 si no existe.
2. Comprueba la configuración de Wokwi (wokwi.toml y diagram.json).
3. Lanza la simulación local mediante wokwi-cli si está instalado, o indica
   cómo ejecutarla mediante el plugin de PyCharm / VS Code.
"""

import os
import shutil
import subprocess
import sys
from pathlib import Path

BASE_DIR = Path(__file__).resolve().parent
TARGET_ELF = BASE_DIR / "target" / "xtensa-esp32-none-elf" / "debug" / "smartpot-iot"
WOKWI_TOML = BASE_DIR / "wokwi.toml"
DIAGRAM_JSON = BASE_DIR / "diagram.json"


def ensure_binary_built():
    """Verifica si el binario ELF existe; si no, lo compila con la toolchain esp."""
    if TARGET_ELF.exists():
        print(f"[OK] Binario firmware encontrado: {TARGET_ELF}")
        return True

    print("[INFO] Binario no encontrado. Compilando con cargo +esp...")
    try:
        cmd = ["cargo", "+esp", "build", "--bin", "smartpot-iot", "--ignore-rust-version"]
        subprocess.run(cmd, cwd=BASE_DIR, check=True)
        print(f"[OK] Compilación exitosa: {TARGET_ELF}")
        return True
    except (subprocess.CalledProcessError, FileNotFoundError) as e:
        print(f"[ERROR] No se pudo compilar el firmware con cargo +esp: {e}")
        return False


def run_wokwi_simulation():
    """Ejecuta la simulación local en Wokwi CLI o muestra instrucciones para el IDE."""
    if not WOKWI_TOML.exists() or not DIAGRAM_JSON.exists():
        sys.exit("[ERROR] Faltan archivos requeridos: wokwi.toml o diagram.json")

    wokwi_cli = shutil.which("wokwi-cli")
    if wokwi_cli:
        print(f"[INFO] Iniciando simulación local con wokwi-cli...")
        try:
            subprocess.run([wokwi_cli, "."], cwd=BASE_DIR, check=True)
            return
        except subprocess.CalledProcessError as err:
            sys.exit(f"[ERROR] wokwi-cli finalizó con error: {err}")

    print("\n" + "=" * 65)
    print(" SmartPot-IoT-Rust · Simulación en Wokwi Lista")
    print("=" * 65)
    print("Archivos de simulación preparados:")
    print(f"  * diagram.json: {DIAGRAM_JSON}")
    print(f"  * wokwi.toml:   {WOKWI_TOML}")
    print(f"  * ELF binary:   {TARGET_ELF}")
    print("\nOpciones para iniciar la simulación:")
    print("  1. En PyCharm / CLion (JetBrains):")
    print("     - Instala el plugin 'Wokwi Simulator'.")
    print("     - Abre 'diagram.json' y presiona el botón verde de 'Play' (Start Simulation).")
    print("\n  2. En VS Code:")
    print("     - Instala la extensión 'Wokwi Simulator'.")
    print("     - Presiona F1 -> 'Wokwi: Start Simulator'.")
    print("\n  3. Mediante Wokwi CLI:")
    print("     - Instala: npm install -g @wokwi/cli  (o cargo install wokwi-cli)")
    print("     - Ejecuta: wokwi-cli .")
    print("=" * 65 + "\n")


if __name__ == "__main__":
    if ensure_binary_built():
        run_wokwi_simulation()
