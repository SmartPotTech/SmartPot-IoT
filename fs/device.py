"""Ciclo del dispositivo: mide, muestra y apaga a tiempo siempre; publica y obedece solo con conexión al broker.

No depende del hardware: main.py le entrega los sensores, los actuadores, la pantalla, el cliente MQTT y el enlace
(WiFi y hora), y llama a step() cada medio segundo.
"""

RETRY_SECONDS = 10
WIFI_RETRY_SECONDS = 30


class Device:
    def __init__(self, read_sensors, bank, lcd, client, link, interval_seconds, secure=True, report=None):
        self.read_sensors = read_sensors
        self.bank = bank
        self.lcd = lcd
        self.client = client
        self.link = link
        self.interval = interval_seconds
        # Con TLS hace falta la hora sincronizada para validar el certificado del broker.
        self.secure = secure
        self.report = report
        self.online = False
        self.next_attempt = 0
        self.next_reading = 0

    def step(self, now):
        self.bank.tick(now)
        if not self.online:
            self._reconnect(now)
        if self.online:
            try:
                self.client.poll()
            except OSError as error:
                self._lost(error, now)
        if now >= self.next_reading:
            self.next_reading = now + self.interval
            self._measure(now)

    def _reconnect(self, now):
        if self.client is None or now < self.next_attempt:
            return
        if not self.link.wifi_up():
            self.next_attempt = now + WIFI_RETRY_SECONDS
            self.link.reconnect_wifi()
            return
        self.next_attempt = now + RETRY_SECONDS
        if self.secure and not self.link.clock_ready() and not self.link.sync_time():
            return
        try:
            self.client.connect()
        except OSError as error:
            print("Sin conexión con el broker:", error)
            self.client.close()
            return
        self.online = True

    def _measure(self, now):
        readings = self.read_sensors()
        if self.report:
            self.report(readings)
        self.lcd.show_readings(readings, self.bank.states(), self.online)
        if self.online:
            try:
                self.client.publish_telemetry(readings)
            except OSError as error:
                self._lost(error, now)

    def _lost(self, error, now):
        print("Conexión perdida:", error)
        self.online = False
        self.client.close()
        self.next_attempt = now + RETRY_SECONDS
