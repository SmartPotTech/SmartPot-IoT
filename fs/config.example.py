# Copia este archivo como config.py (no se versiona) y completa los datos del cultivo.
# El id del cultivo y la clave del dispositivo se obtienen en SmartPot al crear el cultivo
# o en «Dispositivo → Generar nueva clave».

WIFI = {
    "ssid": "Wokwi-GUEST",
    "password": "",
}

SMARTPOT = {
    "crop_id": "<ID_DEL_CULTIVO>",
    "device_key": "<CLAVE_DEL_DISPOSITIVO>",
    "host": "mqtt.smartpot.app",
    "port": 8883,
    "tls": True,
    "ca_file": "ca.crt",
    "interval_seconds": 30,
}
