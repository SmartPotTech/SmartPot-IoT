import subprocess

comando = ["python", "-m", "mpremote", "connect", "port:rfc2217://localhost:4000", "run", "main.py"]
#comando = ["python", "-m", "mpremote", "connect","list"]

try:
    subprocess.run(comando, check=True)
    print("Comando ejecutado exitosamente.")
except subprocess.CalledProcessError as e:
    print(f"Hubo un error al ejecutar el comando: {e}")
except FileNotFoundError:
    print("No se encontró el archivo o el comando. Asegúrate de tener mpremote instalado y en el PATH.")
