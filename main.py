from time import sleep
import network
import utelegram


# Importar Credenciales Wifi y Token Telegram

from config import utelegram_config, wifi_config, usmartpot_config

# Importar clases de sensores
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

# Importar funciones
from utilitys import (
    print_table,
    send_msg,
    send_record
)

# Importar LCD Display
from display import LCDDisplay

# Importar manejador
import usmartpot

# Controlar si se debe enviar el mensaje
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
    water_pump = WaterPump(19)
    uv_light = UVLight(18)

    # Instancia de LCDDisplay con pines scl y sda
    lcd_display = LCDDisplay(scl_pin=16, sda_pin=17)  # Pines I2C para scl y sda

    # Mostrar mensaje de conexión WiFi
    lcd_display.show_wifi_connecting()

    # Configuración inicial de WiFi y pines
    sta_if = network.WLAN(network.STA_IF)
    sta_if.active(True)
    sta_if.connect(wifi_config['ssid'], wifi_config['password'])

    # Definir Bot
    bot = None

    # Espera activa hasta 20 segundos para conectar
    for _ in range(80):
        if sta_if.isconnected():
            print('WiFi Conectado')
            lcd_display.show_online_status()
            try:
                bot = usmartpot.Ubot(usmartpot_config['email'],usmartpot_config['password'])
                should_send_msg = True
                print('Bot inicializado correctamente')
            except Exception as e:
                print(f"Error al inicializar el bot: {e}")
            break
        print('Conectando a WiFi...')
        lcd_display.show_wifi_spinner()
        sleep(0.25)
    else:
        print('No conectado - abortando')


    # Mostrar en Display LCD
    lcd_display.show_welcome_message()
    sleep(5)

    # Enviar datos de los sensores cada 60 segundos
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

        # Mostrar los datos en el display LCD
        lcd_display.display_sensor_data(temp, humidity_air, light, ph, tds, humidity_soil)

        # Enviar mensaje si el bot está conectado y la variable `should_send_msg` es True
        if should_send_msg:
            send_record(bot, usmartpot_config['crop'], temp, humidity_air, light, ph, tds, humidity_soil)
            sleep(60)
        else:
            sleep(60) # Esperar 60 segundos si el bot no fue iniciado
