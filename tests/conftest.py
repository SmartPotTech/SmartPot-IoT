"""Módulos de MicroPython simulados para probar el firmware con CPython."""

import pathlib
import sys
import types

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent / "fs"))


class FakePin:
    OUT = 1
    IN = 0

    def __init__(self, number, mode=None):
        self.number = number
        self.level = 0

    def on(self):
        self.level = 1

    def off(self):
        self.level = 0

    def value(self, level=None):
        if level is None:
            return self.level
        self.level = level


class FakeADC:
    ATTN_11DB = 3
    readings: dict[int, int] = {}

    def __init__(self, pin):
        self.pin = pin

    def atten(self, value):
        self.attenuation = value

    def read_u16(self):
        value = FakeADC.readings.get(self.pin.number, 0)
        if value is None:
            raise OSError("sin lectura")
        return value


class FakeDHT22:
    fail = False

    def __init__(self, pin):
        self.pin = pin

    def measure(self):
        if FakeDHT22.fail:
            raise OSError("DHT22 no responde")

    def temperature(self):
        return 23.5

    def humidity(self):
        return 61.0


class FakeMQTTClient:
    """umqtt.simple de MicroPython 1.23 o posterior: ssl es un SSLContext y no existe ssl_params."""

    def __init__(self, client_id, server, port=0, user=None, password=None, keepalive=0, ssl=None):
        self.client_id = client_id
        self.server = server
        self.options = {"port": port, "user": user, "password": password, "ssl": ssl}
        self.published = []
        self.subscriptions = []
        self.will = None
        self.callback = None
        self.pending = []

    def set_callback(self, callback):
        self.callback = callback

    def set_last_will(self, topic, message, retain=False, qos=0):
        self.will = (topic, message, retain, qos)

    def connect(self):
        return 0

    def publish(self, topic, message, retain=False, qos=0):
        self.published.append((topic, message, retain, qos))

    def subscribe(self, topic, qos=0):
        self.subscriptions.append((topic, qos))

    def check_msg(self):
        while self.pending:
            topic, message = self.pending.pop(0)
            self.callback(topic, message)

    def disconnect(self):
        pass


class LegacyMQTTClient(FakeMQTTClient):
    """umqtt.simple de MicroPython 1.22: ssl=True y los parámetros de ssl.wrap_socket en ssl_params."""

    def __init__(self, client_id, server, port=0, user=None, password=None, keepalive=0, ssl=False, ssl_params=None):
        super().__init__(client_id, server, port, user, password, keepalive)
        self.options.update({"ssl": ssl, "ssl_params": ssl_params})


class FakeLcd:
    """I2cLcd que guarda lo que se escribe en cada fila."""

    def __init__(self, i2c=None, address=0x27, rows=4, cols=20):
        self.rows = [" " * cols for _ in range(rows)]
        self.cols = cols
        self.glyphs = {}
        self.x = self.y = 0

    def clear(self):
        self.rows = [" " * self.cols for _ in self.rows]

    def move_to(self, x, y):
        self.x, self.y = x, y

    def putstr(self, text):
        row = self.rows[self.y]
        self.rows[self.y] = (row[:self.x] + text + row[self.x + len(text):])[:self.cols]
        self.x += len(text)

    def custom_char(self, location, charmap):
        self.glyphs[location] = list(charmap)


machine = types.ModuleType("machine")
machine.Pin = FakePin
machine.ADC = FakeADC
machine.I2C = lambda *args, **kwargs: None
dht = types.ModuleType("dht")
dht.DHT22 = FakeDHT22
umqtt = types.ModuleType("umqtt")
umqtt_simple = types.ModuleType("umqtt.simple")
umqtt_simple.MQTTClient = FakeMQTTClient
umqtt.simple = umqtt_simple
ntptime = types.ModuleType("ntptime")
ntptime.settime = lambda: None
i2c_lcd = types.ModuleType("i2c_lcd")
i2c_lcd.I2cLcd = FakeLcd

sys.modules.update({"machine": machine, "dht": dht, "umqtt": umqtt, "umqtt.simple": umqtt_simple,
                    "ntptime": ntptime, "i2c_lcd": i2c_lcd})
