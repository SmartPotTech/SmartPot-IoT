from machine import ADC, Pin

class Sensor:
    def __init__(self, pin_num, adc_bits=12, attenuation=ADC.ATTN_11DB):
        self.pin = Pin(pin_num)
        self.adc = ADC(self.pin)
        self.adc.width(adc_bits)  # Configurar la resolución del ADC
        self.adc.atten(attenuation)  # Configurar la atenuación

    def read_value(self):
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