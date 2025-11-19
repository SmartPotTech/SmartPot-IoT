from time import localtime
import network
import ntptime
import time

# Códigos de color ANSI para impresión en consola
RED = '\033[91m'
GREEN = '\033[92m'
YELLOW = '\033[93m'
BLUE = '\033[94m'
MAGENTA = '\033[95m'
CYAN = '\033[96m'
RESET = '\033[0m'

def sync_time():
    print("Sincronizando tiempo NTP...")
    try:
        ntptime.settime()
        time.sleep(1)
        print("Tiempo sincronizado:", localtime())
    except Exception as e:
        print("No se pudo sincronizar tiempo:", e)


# Función para formatear la fecha y hora actual
def get_current_time():
    current_time = localtime()
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
