"""Cliente MQTT del dispositivo según el contrato v1 de SmartPot.

Las funciones de formato no dependen del hardware y se prueban con CPython.
"""

import json

TOPIC_PREFIX = "smartpot/v1"
TELEMETRY_FIELDS = ("temperature", "humidity", "brightness", "ph", "tds", "soilMoisture", "atmosphere")
DECIMALS = {"brightness": 0, "tds": 0, "ph": 2}


def topics(crop_id, prefix=TOPIC_PREFIX):
    base = prefix + "/" + crop_id
    return {
        "telemetry": base + "/telemetry",
        "commands": base + "/commands",
        "ack": base + "/commands/ack",
        "status": base + "/status",
    }


def telemetry_payload(readings):
    data = {}
    for field in TELEMETRY_FIELDS:
        value = readings.get(field)
        if value is not None:
            data[field] = round(value, DECIMALS.get(field, 1))
    return json.dumps(data)


def parse_command(payload):
    try:
        if isinstance(payload, (bytes, bytearray)):
            payload = payload.decode()
        command = json.loads(payload)
        return {
            "id": str(command["id"]),
            "actuator": str(command.get("actuator", "")).upper(),
            "action": str(command.get("action", "")).upper(),
            "durationSeconds": command.get("durationSeconds"),
        }
    except (ValueError, KeyError, TypeError):
        return None


def ack_payload(command_id, executed, message):
    return json.dumps({"id": command_id, "status": "EXECUTED" if executed else "FAILED", "message": message})


def _ssl_context(ca_data):
    import ssl

    context = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
    context.verify_mode = ssl.CERT_REQUIRED
    context.load_verify_locations(cadata=ca_data)
    return context


class SmartPotClient:
    def __init__(self, crop_id, device_key, host, port, use_tls, ca_file, on_command, client_factory=None):
        self.crop_id = crop_id
        self.topics = topics(crop_id)
        self.on_command = on_command
        if client_factory is None:
            from umqtt.simple import MQTTClient

            client_factory = MQTTClient
        options = {"port": port, "user": crop_id, "password": device_key, "keepalive": 60}
        if use_tls:
            try:
                options["ssl"] = _ssl_context(ca_file)
            except (AttributeError, OSError) as error:
                # MicroPython anterior a 1.23 no trae SSLContext: se cifra sin verificar la CA.
                print("Aviso: TLS sin verificación de la CA ({})".format(error))
                options["ssl"] = True
                options["ssl_params"] = {"server_hostname": host}
        self.client = client_factory("smartpot-" + crop_id, host, **options)
        self.client.set_callback(self._on_message)

    def connect(self):
        self.client.set_last_will(self.topics["status"], b"offline", retain=True, qos=1)
        self.client.connect()
        self.client.publish(self.topics["status"], b"online", retain=True, qos=1)
        self.client.subscribe(self.topics["commands"], qos=1)
        print("MQTT conectado como", self.crop_id)

    def poll(self):
        self.client.check_msg()

    def publish_telemetry(self, readings):
        self.client.publish(self.topics["telemetry"], telemetry_payload(readings).encode())

    def acknowledge(self, command_id, executed, message):
        self.client.publish(self.topics["ack"], ack_payload(command_id, executed, message).encode(), qos=1)

    def disconnect(self):
        try:
            self.client.publish(self.topics["status"], b"offline", retain=True, qos=1)
            self.client.disconnect()
        except OSError:
            pass

    def _on_message(self, topic, message):
        command = parse_command(message)
        if command is None:
            print("Comando ilegible descartado")
            return
        self.on_command(command)
