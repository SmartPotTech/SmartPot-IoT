"""Firmware de SmartPot para un cultivo real: ESP32 con MicroPython (Wokwi o placa física)."""

import time

import network

import config
import utils
from actuators import Actuator, ActuatorBank
from device import Device
from display import LCDDisplay
from sensors import AtmosphereSensor, LightSensor, PHSensor, SoilMoistureSensor, TDSSensor
from smartpot_client import SmartPotClient

POLL_SECONDS = 0.5
WIFI_WAIT_SECONDS = 30


class Link:
    """WiFi y hora del ESP32, para que Device sepa cuándo puede conectarse al broker."""

    def __init__(self):
        self.station = network.WLAN(network.STA_IF)
        self.station.active(True)

    def wifi_up(self):
        return self.station.isconnected()

    def reconnect_wifi(self):
        try:
            self.station.connect(config.WIFI["ssid"], config.WIFI["password"])
        except OSError as error:
            print("WiFi:", error)

    def clock_ready(self):
        return utils.clock_ready()

    def sync_time(self):
        return utils.sync_time()


def connect_wifi(link, lcd):
    """Primer intento al encender, con el giro en la pantalla; si no llega, el ciclo sigue midiendo y reintenta."""
    lcd.show_message("Conectando WiFi", config.WIFI["ssid"])
    link.reconnect_wifi()
    for _ in range(WIFI_WAIT_SECONDS * 4):
        if link.wifi_up():
            print("WiFi conectado:", link.station.ifconfig()[0])
            return True
        lcd.show_spinner()
        time.sleep(0.25)
    print("Sin WiFi: el dispositivo sigue midiendo y reintenta cada 30 s")
    return False


def read_sensors(atmosphere, analog):
    readings = atmosphere.read()
    for name, sensor in analog.items():
        readings[name] = sensor.read()
    return readings


def build_client(settings, lcd, on_command):
    """Sin una CA válida no se envía la clave: el dispositivo queda midiendo sin conexión."""
    try:
        return SmartPotClient(settings["crop_id"], settings["device_key"], settings["host"], settings["port"],
                              settings["tls"], getattr(config, "BROKER", {}).get("CA_CRT"), on_command)
    except (ValueError, OSError) as error:
        print("No se puede conectar con seguridad:", error)
        lcd.show_message("Revisa la CA en", "config.py")
        time.sleep(5)
        return None


def main():
    lcd = LCDDisplay(scl_pin=16, sda_pin=17)
    lcd.show_message("Bienvenido a", "SmartPot ESP32")
    time.sleep(2)
    atmosphere = AtmosphereSensor(15)
    analog = {
        "brightness": LightSensor(34),
        "ph": PHSensor(35),
        "tds": TDSSensor(32),
        "soilMoisture": SoilMoistureSensor(33),
    }
    bank = ActuatorBank({
        "WATER_PUMP": Actuator("Bomba de agua", 19, feminine=True),
        "UV_LIGHT": Actuator("Luz ultravioleta", 18, feminine=True),
        "FAN": Actuator("Ventilador", 5),
    })

    link = Link()
    if connect_wifi(link, lcd):
        utils.sync_time()

    settings = config.SMARTPOT
    client = None

    def on_command(command):
        executed, message = bank.execute(command, time.time())
        print("Comando", command["actuator"], command["action"], "->", message)
        client.acknowledge(command["id"], executed, message)

    client = build_client(settings, lcd, on_command)
    device = Device(lambda: read_sensors(atmosphere, analog), bank, lcd, client, link,
                    settings.get("interval_seconds", 30), secure=settings["tls"], report=utils.print_table)
    while True:
        try:
            device.step(time.time())
        except Exception as error:
            # Un error inesperado no debe dejar un actuador encendido sin control: el ciclo sigue.
            print("Error en el ciclo:", error)
        time.sleep(POLL_SECONDS)


main()
