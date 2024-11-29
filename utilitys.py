from time import localtime

# Códigos de color ANSI para impresión en consola
RED = '\033[91m'
GREEN = '\033[92m'
YELLOW = '\033[93m'
BLUE = '\033[94m'
MAGENTA = '\033[95m'
CYAN = '\033[96m'
RESET = '\033[0m'


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


def send_record(ubot, crop_id, temp, humidity_air, light, ph, tds, humidity_soil):
    """Envía un registro con los datos de sensores a la API SmartPot"""

    # Crear las medidas
    measures = {
        "atmosphere": str(temp),
        "brightness": str(light),
        "temperature": str(temp),
        "ph": str(ph),
        "tds": str(tds),
        "humidity": str(humidity_air)
    }

    try:
        response_data = ubot.create_record(crop_id, measures)

        if response_data:
            print("Registro enviado exitosamente:")
            print(response_data)
        else:
            print("No se recibió respuesta del servidor.")

    except Exception as e:
        print(f"Error al enviar el registro: {e}")
