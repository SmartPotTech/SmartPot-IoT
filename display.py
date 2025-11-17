from i2c_lcd import I2cLcd
from machine import I2C, Pin


class LCDDisplay:
    def __init__(self, scl_pin, sda_pin, address=0x27, rows=4, cols=20, freq=400000):
        # Configuración del bus I2C y el LCD
        self.i2c = I2C(scl=Pin(scl_pin), sda=Pin(sda_pin), freq=freq)
        self.lcd = I2cLcd(self.i2c, address, rows, cols)
        self.glyphs = "\xa1\xa5\xdb"
        self.position = 0

    def print(self, text, row, col):
        self.lcd.move_to(col, row)
        self.lcd.putstr(text)

    def show_welcome_message(self):
        self.lcd.clear()
        self.print("Welcome to", 1, 4)
        self.print("SmartPot ESP32", 2, 2)

    def show_wifi_connecting(self):
        self.lcd.clear()
        self.print("Connecting to", 1, 2)
        self.print("WiFi", 2, 6)

    def show_wifi_spinner(self):
        self.print(self.glyphs[self.position], 2, 15)
        self.position = (self.position + 1) % len(self.glyphs)

    def show_online_status(self):
        self.lcd.clear()
        self.print("Online", 1, 6)

    def display_sensor_data(self, temp, humidity_air, light, ph, tds, humidity_soil):
        self.lcd.clear()
        self.print(f"Amb {temp:3.0f} C ^ {humidity_air:3.0f}%", 0, 0)  # Línea 1
        self.print(f"Luz {light:3.0f} lux", 1, 0)  # Línea 2
        self.print(f"Hum {humidity_soil:3.0f}% ^ pH {ph:3.0f}", 2, 0)  # Línea 3
        self.print(f"TDS {tds:4.0f} ppm", 3, 0)  # Línea 4
