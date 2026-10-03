import time

import ntptime

LABELS = (
    ("temperature", "Temperatura", "°C"),
    ("humidity", "Humedad aire", "%"),
    ("brightness", "Luz", "lux"),
    ("ph", "pH", ""),
    ("tds", "TDS", "ppm"),
    ("soilMoisture", "Humedad sustr.", "%"),
)
# El ESP32 arranca en el año 2000: antes de sincronizar por NTP la hora no sirve para validar certificados.
SYNCED_YEAR = 2025


def clock_ready(moment=None):
    return (moment or time.gmtime())[0] >= SYNCED_YEAR


def sync_time():
    """Sincroniza el reloj por NTP; TLS necesita la hora correcta para validar el certificado."""
    for _ in range(3):
        try:
            ntptime.settime()
            print("Hora sincronizada:", time.localtime())
            return True
        except OSError as error:
            print("No se pudo sincronizar la hora:", error)
            time.sleep(2)
    return False


def timestamp(moment):
    if not clock_ready(moment):
        return "hora sin sincronizar"
    return "{}-{:02d}-{:02d} {:02d}:{:02d}:{:02d} UTC".format(*moment[:6])


def format_row(label, value, unit):
    text = "--" if value is None else "{:.2f} {}".format(value, unit).strip()
    return "| {:<14} | {:<14} |".format(label, text)


def print_table(readings, moment=None):
    line = "+" + "-" * 16 + "+" + "-" * 16 + "+"
    print(line)
    print("| {:<31} |".format("Lectura " + timestamp(moment or time.gmtime())))
    print(line)
    for key, label, unit in LABELS:
        print(format_row(label, readings.get(key), unit))
    print(line)
