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


def check_ca(ca_data):
    """La CA del broker en PEM, como la trae BROKER["CA_CRT"] de config.py; sin ella no se envía la clave."""
    if not isinstance(ca_data, str) or "-----BEGIN CERTIFICATE-----" not in ca_data:
        raise ValueError("config.py no trae la CA del broker: copia el bloque BROKER de la guía de conexión")
    return ca_data


def tls_client(factory, client_id, host, options, ca_data):
    """Cliente MQTT sobre TLS que verifica el certificado del broker con la CA y su nombre.

    umqtt.simple de MicroPython 1.22 recibe ssl=True y ssl_params; si recibe un SSLContext lo ignora y cifra sin
    verificar. Desde 1.23 recibe el SSLContext y rechaza ssl_params, por eso se prueba primero la forma de 1.22.
    """
    import ssl

    check_ca(ca_data)
    try:
        return factory(client_id, host, ssl=True, ssl_params={
            "server_hostname": host, "cert_reqs": ssl.CERT_REQUIRED, "cadata": ca_data}, **options)
    except TypeError:
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
        context.verify_mode = ssl.CERT_REQUIRED
        context.load_verify_locations(cadata=ca_data)
        return factory(client_id, host, ssl=context, **options)


class SmartPotClient:
    def __init__(self, crop_id, device_key, host, port, use_tls, ca_data, on_command, client_factory=None):
        self.crop_id = crop_id
        self.topics = topics(crop_id)
        self.on_command = on_command
        if client_factory is None:
            from umqtt.simple import MQTTClient

            client_factory = MQTTClient
        options = {"port": port, "user": crop_id, "password": device_key, "keepalive": 60}
        client_id = "smartpot-" + crop_id
        if use_tls:
            self.client = tls_client(client_factory, client_id, host, options, ca_data)
        else:
            self.client = client_factory(client_id, host, **options)
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

    def close(self):
        """Suelta el socket de una conexión caída antes de volver a conectar."""
        sock = getattr(self.client, "sock", None)
        if sock is None:
            return
        try:
            sock.close()
        except OSError as error:
            print("El socket ya estaba cerrado:", error)

    def _on_message(self, topic, message):
        command = parse_command(message)
        if command is None:
            print("Comando ilegible descartado")
            return
        self.on_command(command)
