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
    def __init__(self, client_id, server, port=0, user=None, password=None, keepalive=0, ssl=None, ssl_params=None):
        self.client_id = client_id
        self.server = server
        self.options = {"port": port, "user": user, "password": password, "ssl": ssl, "ssl_params": ssl_params}
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


machine = types.ModuleType("machine")
machine.Pin = FakePin
machine.ADC = FakeADC
machine.I2C = object
dht = types.ModuleType("dht")
dht.DHT22 = FakeDHT22
umqtt = types.ModuleType("umqtt")
umqtt_simple = types.ModuleType("umqtt.simple")
umqtt_simple.MQTTClient = FakeMQTTClient
umqtt.simple = umqtt_simple
ntptime = types.ModuleType("ntptime")
ntptime.settime = lambda: None

sys.modules.update({"machine": machine, "dht": dht, "umqtt": umqtt, "umqtt.simple": umqtt_simple,
                    "ntptime": ntptime})
