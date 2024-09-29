from machine import ADC, Pin
from time import sleep, localtime
import network
import utelegram
from config import utelegram_config, wifi_config

# Códigos de color ANSI para impresión en consola
RED = '\033[91m'
GREEN = '\033[92m'
YELLOW = '\033[93m'
BLUE = '\033[94m'
MAGENTA = '\033[95m'
CYAN = '\033[96m'
RESET = '\033[0m'

class Sensor:
    def __init__(self, pin_num, adc_bits=12, attenuation=ADC.ATTN_11DB):
        self.pin = Pin(pin_num)
        self.adc = ADC(self.pin)
        self.adc.width(adc_bits)  # Configurar la resolución del ADC
        self.adc.atten(attenuation)  # Configurar la atenuación

    def read_value(self):
        sleep(0.1)  # Esperar un breve período de tiempo antes de leer el valor
        try:
            value = self.adc.read()
            print(f"Raw value from pin GPIO{self.pin}: {value}")
            return value
        except OSError as e:
            print(f"Error reading value on GPIO{self.pin}: {e}")
            return None

    def map_value(self, value, from_low, from_high, to_low, to_high):
        return (value - from_low) * (to_high - to_low) / (from_high - from_low) + to_low

# Clases para cada sensor
class TempSensor(Sensor):
    def read_temp(self):
        raw_value = self.read_value()
        if raw_value is not None:
            return self.map_value(raw_value, 0, 4095, -40, 125)
        else:
            print("Failed to read Temp value")
            return None

class HumidityAirSensor(Sensor):
    def read_humidity(self):
        raw_value = self.read_value()
        if raw_value is not None:
            return self.map_value(raw_value, 0, 4095, 0, 100)
        else:
            print("Failed to read Humidity Air value")
            return None

class LightSensor(Sensor):
    def read_light(self):
        raw_value = self.read_value()
        if raw_value is not None:
            return self.map_value(raw_value, 0, 4095, 0, 1000)
        else:
            print("Failed to read Light value")
            return None

class PHSensor(Sensor):
    def read_ph(self):
        raw_value = self.read_value()
        if raw_value is not None:
            return self.map_value(raw_value, 0, 4095, 0, 14)
        else:
            print("Failed to read pH value")
            return None

class TDSSensor(Sensor):
    def read_tds(self):
        raw_value = self.read_value()
        if raw_value is not None:
            return self.map_value(raw_value, 0, 4095, 0, 1000)
        else:
            print("Failed to read TDS value")
            return None

class HumiditySoilSensor(Sensor):
    def read_soil_humidity(self):
        raw_value = self.read_value()
        if raw_value is not None:
            return self.map_value(raw_value, 0, 4095, 0, 100)
        else:
            print("Failed to read Soil Humidity value")
            return None

# Función para formatear la fecha y hora actual
def get_current_time():
    current_time = localtime()  # Obtiene la hora local
    return f"{current_time[0]}-{current_time[1]:02}-{current_time[2]:02} {current_time[3]:02}:{current_time[4]:02}:{current_time[5]:02}"

def print_table(temp, humidity_air, light, ph, tds, humidity_soil):
    # Encabezado de la tabla
    print(f"\n+--------------+------------------------+")
    print(f"|         " + get_current_time() + "           |")
    print(f"+--------------+------------------------+")
    print(f"| Sensor       | Valor                  |")
    print(f"+--------------+------------------------+")
    
    # Fila para temperatura
    if temp is not None:
        temp_display = f"{RED}{temp:7.2f}°C{RESET:<17}"
    else:
        temp_display = f"{RED}N/A{RESET:<17}"
    print(f"| Temp         | {temp_display} |")
    
    # Fila para humedad del aire
    if humidity_air is not None:
        humidity_air_display = f"{BLUE}{humidity_air:7.2f}%{RESET:<18}"
    else:
        humidity_air_display = f"{BLUE}N/A{RESET:<18}"
    print(f"| Humedad Aire | {humidity_air_display} |")
    
    # Fila para luz
    if light is not None:
        light_display = f"{YELLOW}{light:7.2f} lux{RESET:<15}"
    else:
        light_display = f"{YELLOW}N/A{RESET:<15}"
    print(f"| Luz          | {light_display} |")
    
    # Fila para pH
    if ph is not None:
        ph_display = f"{GREEN}{ph:7.2f}{RESET:<19}"
    else:
        ph_display = f"{GREEN}N/A{RESET:<19}"
    print(f"| pH           | {ph_display} |")
    
    # Fila para TDS
    if tds is not None:
        tds_display = f"{CYAN}{tds:7.2f} ppm{RESET:<15}"
    else:
        tds_display = f"{CYAN}N/A{RESET:<15}"
    print(f"| TDS          | {tds_display} |")
    
    # Fila para humedad del suelo
    if humidity_soil is not None:
        humidity_soil_display = f"{MAGENTA}{humidity_soil:7.2f}%{RESET:<18}"
    else:
        humidity_soil_display = f"{MAGENTA}N/A{RESET:<18}"
    print(f"| Humedad Suelo| {humidity_soil_display} |")
    
    # Línea de cierre
    print(f"+--------------+------------------------+\n")

def send_msg(bot, chat_id, temp, humidity_air, light, ph, tds, humidity_soil):
    # Crear mensaje con los datos de los sensores
    sensor_data = (
        f"*Sensor Data:* \n",
        f"*Temp:* {temp:.2f}°C\n" if temp is not None else "*Temp:* N/A\n",
        f"*Humedad Aire:* {humidity_air:.2f}%\n" if humidity_air is not None else "*Humedad Aire:* N/A\n",
        f"*Luz:* {light:.2f} lux\n" if light is not None else "*Luz:* N/A\n",
        f"*pH:* {ph:.2f}\n" if ph is not None else "*pH:* N/A\n",
        f"*TDS:* {tds:.2f} ppm\n" if tds is not None else "*TDS:* N/A\n",
        f"*Humedad Suelo:* {humidity_soil:.2f}%\n" if humidity_soil is not None else "*Humedad Suelo:* N/A\n"
    )

    # Unir la lista en una sola cadena
    formatted_message = "".join(sensor_data)

    try:
        bot.send(chat_id, formatted_message)
        print("Mensaje enviado correctamente")
    except Exception as e:
        print(f"Error al enviar el mensaje: {e}")

# Variable global para controlar si se debe enviar el mensaje
should_send_msg = False

if __name__ == "__main__":
    # Instancia de cada sensor
    temp_sensor = TempSensor(34)
    humidity_air_sensor = HumidityAirSensor(35)
    light_sensor = LightSensor(32)
    ph_sensor = PHSensor(33)
    tds_sensor = TDSSensor(27)
    humidity_soil_sensor = HumiditySoilSensor(4)

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
    while True:
        # Leer valores de los sensores
        temp = temp_sensor.read_temp()
        humidity_air = humidity_air_sensor.read_humidity()
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
