from machine import ADC, Pin, I2C
from time import sleep, localtime
import network
import utelegram
from config import utelegram_config, wifi_config
from i2c_lcd import I2cLcd

# Importar clases de sensores desde sensors.py
from sensors import (
    LightSensor,
    PHSensor,
    TDSSensor,
    HumiditySoilSensor,
    AtmosphereSensor,
)

from actuators import (
    WaterPump,
    UVLight,

)

# Importar funciones desde utilitys.py
from utilitys import (
    print_table,
    send_msg
)

# Variable global para controlar si se debe enviar el mensaje
should_send_msg = False

if __name__ == "__main__":
    # Instancia de cada sensor digital
    dht_sensor = AtmosphereSensor(15)
    # Instancia de cada sensor ADC
    light_sensor = LightSensor(34)
    ph_sensor = PHSensor(35)
    tds_sensor = TDSSensor(32)
    humidity_soil_sensor = HumiditySoilSensor(33)
    # Instancia de actuadores
    water_pump = WaterPump(18)
    uv_light = UVLight(19)

    # Instancia de LCD
    AddressOfLcd = 0x27
    i2c = I2C(scl=Pin(16), sda=Pin(17), freq=400000) # connect scl to GPIO 22, sda to GPIO 21
    lcd = I2cLcd(i2c, AddressOfLcd, 2, 16)


    # Configuración inicial de WiFi y pines
    sta_if = network.WLAN(network.STA_IF)
    sta_if.active(True)
    sta_if.connect(wifi_config['ssid'], wifi_config['password'])

    # Definir bot aquí
    bot = None

    # Espera activa hasta 20 segundos para conectar
    for _ in range(20):
        if sta_if.isconnected():
            print('WiFi Conectado')
            try:
                bot = utelegram.ubot(utelegram_config['token'])
                should_send_msg = True  # Cambia el estado de envío de mensajes
                print('Bot inicializado correctamente')
            except Exception as e:
                print(f"Error al inicializar el bot: {e}")
            break
        print('Conectando a WiFi...')
        sleep(1)
    else:
        print('No conectado - abortando')

    # Enviar datos de los sensores cada 60 segundos
    water_pump.turn_on()
    uv_light.turn_on()
    # Mostrar en Display LCD
    lcd.clear()
    lcd.putstr("SmartPot ESP32")
    while True:
        # Leer valores de los sensores
        dht_data = dht_sensor.read_value()
        temp = dht_data["temperature"]
        humidity_air = dht_data["humidity"]
        light = light_sensor.read_light()
        ph = ph_sensor.read_ph()
        tds = tds_sensor.read_tds()
        humidity_soil = humidity_soil_sensor.read_soil_humidity()

        # Imprimir los valores en formato tabla
        print_table(temp, humidity_air, light, ph, tds, humidity_soil)


        # Enviar mensaje si el bot está conectado y la variable `should_send_msg` es True
        if should_send_msg and bot:
            send_msg(bot, utelegram_config['chat'], temp, humidity_air, light, ph, tds, humidity_soil)

        sleep(60)
