"""Ejecuta el firmware en la simulación de Wokwi (VS Code o wokwi-cli) montando la carpeta fs."""

import subprocess
import sys

COMMAND = [sys.executable, "-m", "mpremote", "connect", "port:rfc2217://localhost:4000",
           "mount", "fs", "exec", "import main"]

if __name__ == "__main__":
    try:
        subprocess.run(COMMAND, check=True)
    except subprocess.CalledProcessError as error:
        sys.exit(f"El firmware terminó con error: {error}")
    except FileNotFoundError:
        sys.exit("No se encontró mpremote: ejecuta «uv sync» antes de iniciar la simulación.")
