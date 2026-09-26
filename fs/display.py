from i2c_lcd import I2cLcd
from machine import I2C, Pin


class LCDDisplay:
    """Pantalla LCD 20x4 por I2C. El controlador HD44780 no tiene tildes: los textos van sin ellas."""

    def __init__(self, scl_pin, sda_pin, address=0x27, rows=4, cols=20, freq=400000):
        self.i2c = I2C(scl=Pin(scl_pin), sda=Pin(sda_pin), freq=freq)
        self.lcd = I2cLcd(self.i2c, address, rows, cols)
        self.glyphs = "|/-\\"
        self.position = 0

    def print(self, text, row, col=0):
        self.lcd.move_to(col, row)
        self.lcd.putstr(text)

    def show_message(self, first, second=""):
        self.lcd.clear()
        self.print(first, 1, max(0, (20 - len(first)) // 2))
        if second:
            self.print(second, 2, max(0, (20 - len(second)) // 2))

    def show_spinner(self):
        self.print(self.glyphs[self.position], 3, 19)
        self.position = (self.position + 1) % len(self.glyphs)

    def show_readings(self, readings, states):
        def value(name, width, decimals=0):
            number = readings.get(name)
            if number is None:
                return " " * (width - 2) + "--"
            return ("{:" + str(width) + "." + str(decimals) + "f}").format(number)

        pump = "B" if states.get("WATER_PUMP") else "-"
        light = "L" if states.get("UV_LIGHT") else "-"
        fan = "V" if states.get("FAN") else "-"
        self.lcd.clear()
        self.print("T {}C H {}%".format(value("temperature", 4, 1), value("humidity", 3)), 0)
        self.print("Luz {} lux".format(value("brightness", 5)), 1)
        self.print("Sust {}% pH {}".format(value("soilMoisture", 3), value("ph", 4, 1)), 2)
        self.print("TDS {}ppm [{}{}{}]".format(value("tds", 4), pump, light, fan), 3)
