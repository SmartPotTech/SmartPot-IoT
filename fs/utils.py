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


def format_row(label, value, unit):
    text = "--" if value is None else "{:.2f} {}".format(value, unit).strip()
    return "| {:<14} | {:<14} |".format(label, text)


def print_table(readings):
    line = "+" + "-" * 16 + "+" + "-" * 16 + "+"
    print(line)
    for key, label, unit in LABELS:
        print(format_row(label, readings.get(key), unit))
    print(line)
