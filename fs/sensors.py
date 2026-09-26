import dht
from machine import ADC, Pin


class ADCSensor:
    """Sensor analógico del ESP32 (potenciómetro en la simulación) con escala lineal."""

    scale = (0.0, 100.0)

    def __init__(self, pin_num):
        self.adc = ADC(Pin(pin_num))
        self.adc.atten(ADC.ATTN_11DB)

    def read(self):
        try:
            raw = self.adc.read_u16()
        except OSError as error:
            print("Error leyendo el sensor analógico:", error)
            return None
        low, high = self.scale
        return low + raw * (high - low) / 65535


class LightSensor(ADCSensor):
    scale = (0.0, 2000.0)


class PHSensor(ADCSensor):
    scale = (0.0, 14.0)


class TDSSensor(ADCSensor):
    scale = (0.0, 3000.0)


class SoilMoistureSensor(ADCSensor):
    scale = (0.0, 100.0)


class AtmosphereSensor:
    """DHT22: temperatura y humedad del aire."""

    def __init__(self, pin_num):
        self.sensor = dht.DHT22(Pin(pin_num))

    def read(self):
        try:
            self.sensor.measure()
            return {"temperature": self.sensor.temperature(), "humidity": self.sensor.humidity()}
        except OSError as error:
            print("Error leyendo el DHT22:", error)
            return {"temperature": None, "humidity": None}
