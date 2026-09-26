"""Firmware de la maceta SmartPot para ESP32 con MicroPython (Wokwi o placa física)."""

import time

import network

import config
import utils
from actuators import Actuator, ActuatorBank
from display import LCDDisplay
from sensors import AtmosphereSensor, LightSensor, PHSensor, SoilMoistureSensor, TDSSensor
from smartpot_client import SmartPotClient

POLL_SECONDS = 0.5
RETRY_SECONDS = 10


def connect_wifi(lcd):
    lcd.show_message("Conectando WiFi", config.WIFI["ssid"])
    station = network.WLAN(network.STA_IF)
    station.active(True)
    station.connect(config.WIFI["ssid"], config.WIFI["password"])
    for _ in range(120):
        if station.isconnected():
            print("WiFi conectado:", station.ifconfig()[0])
            return True
        lcd.show_spinner()
        time.sleep(0.25)
    print("No se pudo conectar al WiFi")
    return False


def read_sensors(atmosphere, analog):
    readings = atmosphere.read()
    for name, sensor in analog.items():
        readings[name] = sensor.read()
    return readings


def main():
    lcd = LCDDisplay(scl_pin=16, sda_pin=17)
    atmosphere = AtmosphereSensor(15)
    analog = {
        "brightness": LightSensor(34),
        "ph": PHSensor(35),
        "tds": TDSSensor(32),
        "soilMoisture": SoilMoistureSensor(33),
    }
    bank = ActuatorBank({
        "WATER_PUMP": Actuator("Bomba", 19),
        "UV_LIGHT": Actuator("Luz UV", 18),
        "FAN": Actuator("Ventilador", 5),
    })

    while not connect_wifi(lcd):
        time.sleep(RETRY_SECONDS)
    utils.sync_time()

    settings = config.SMARTPOT
    client = None

    def on_command(command):
        executed, message = bank.execute(command, time.time())
        print("Comando", command["actuator"], command["action"], "->", message)
        client.acknowledge(command["id"], executed, message)

    client = SmartPotClient(settings["crop_id"], settings["device_key"], settings["host"], settings["port"],
                            settings["tls"], settings["ca_file"], on_command)
    lcd.show_message("Bienvenido a", "SmartPot ESP32")

    connected = False
    next_reading = 0
    while True:
        try:
            if not connected:
                lcd.show_message("Conectando", "broker MQTT")
                client.connect()
                connected = True
                lcd.show_message("En linea")
            client.poll()
            now = time.time()
            bank.tick(now)
            if now >= next_reading:
                readings = read_sensors(atmosphere, analog)
                utils.print_table(readings)
                lcd.show_readings(readings, bank.states())
                client.publish_telemetry(readings)
                next_reading = now + settings["interval_seconds"]
            time.sleep(POLL_SECONDS)
        except OSError as error:
            print("Conexión perdida:", error)
            connected = False
            lcd.show_message("Sin conexion", "reintentando")
            time.sleep(RETRY_SECONDS)


main()
