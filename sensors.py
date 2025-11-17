from machine import ADC, Pin
import dht


class ADCSensor:
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


# Clases para cada sensor ADC
class LightSensor(ADCSensor):
    def read_light(self):
        raw_value = self.read_value()
        if raw_value is not None:
            return self.map_value(raw_value, 0, 4095, 0, 1000)
        else:
            print("Failed to read Light value")
            return None


class PHSensor(ADCSensor):
    def read_ph(self):
        raw_value = self.read_value()
        if raw_value is not None:
            return self.map_value(raw_value, 0, 4095, 0, 14)
        else:
            print("Failed to read pH value")
            return None


class TDSSensor(ADCSensor):
    def read_tds(self):
        raw_value = self.read_value()
        if raw_value is not None:
            return self.map_value(raw_value, 0, 4095, 0, 1000)
        else:
            print("Failed to read TDS value")
            return None


class HumiditySoilSensor(ADCSensor):
    def read_soil_humidity(self):
        raw_value = self.read_value()
        if raw_value is not None:
            return self.map_value(raw_value, 0, 4095, 0, 100)
        else:
            print("Failed to read Soil Humidity value")
            return None


class DigitalSensor:
    def __init__(self, pin_num):
        self.pin = Pin(pin_num)

    def read_value(self):
        raise NotImplementedError("This method should be overridden in subclasses")


class AtmosphereSensor(DigitalSensor):
    def __init__(self, pin_num):
        super().__init__(pin_num)
        self.sensor = dht.DHT22(self.pin)

    def read_value(self):
        try:
            self.sensor.measure()
            temp = self.sensor.temperature()
            humidity = self.sensor.humidity()
            return {"temperature": temp, "humidity": humidity}
        except OSError as e:
            print(f"Error reading DHT22 sensor on GPIO{self.pin}: {e}")
            return None
