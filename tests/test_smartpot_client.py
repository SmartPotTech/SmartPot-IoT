import json

from conftest import FakeMQTTClient
from smartpot_client import SmartPotClient, ack_payload, parse_command, telemetry_payload, topics

CROP = "66f5a1000000000000000101"


def client(received=None, **overrides):
    options = {"crop_id": CROP, "device_key": "clave", "host": "mqtt.smartpot.app", "port": 8883, "use_tls": False,
               "ca_file": "ca.crt", "on_command": (received.append if received is not None else lambda c: None),
               "client_factory": FakeMQTTClient}
    options.update(overrides)
    return SmartPotClient(**options)


def test_topics_follow_the_v1_contract():
    assert topics(CROP)["telemetry"] == f"smartpot/v1/{CROP}/telemetry"
    assert topics(CROP)["ack"] == f"smartpot/v1/{CROP}/commands/ack"


def test_telemetry_skips_missing_sensors_and_rounds():
    payload = json.loads(telemetry_payload({"temperature": 23.456, "ph": 6.1234, "tds": 812.7, "humidity": None}))
    assert payload == {"temperature": 23.5, "ph": 6.12, "tds": 813}


def test_commands_are_parsed_and_invalid_ones_ignored():
    command = parse_command(b'{"id": "c1", "actuator": "water_pump", "action": "activate", "durationSeconds": 15}')
    assert command == {"id": "c1", "actuator": "WATER_PUMP", "action": "ACTIVATE", "durationSeconds": 15}
    assert parse_command(b"no es json") is None
    assert parse_command(b'{"actuator": "FAN"}') is None


def test_acknowledgement_payload():
    assert json.loads(ack_payload("c1", False, "sin bomba")) == {"id": "c1", "status": "FAILED", "message": "sin bomba"}


def test_connect_announces_status_with_last_will_and_subscribes():
    smartpot = client()
    smartpot.connect()
    mqtt = smartpot.client
    assert mqtt.client_id == f"smartpot-{CROP}"
    assert mqtt.options["user"] == CROP
    assert mqtt.will == (f"smartpot/v1/{CROP}/status", b"offline", True, 1)
    assert (f"smartpot/v1/{CROP}/status", b"online", True, 1) in mqtt.published
    assert mqtt.subscriptions == [(f"smartpot/v1/{CROP}/commands", 1)]


def test_incoming_commands_reach_the_handler_and_can_be_acknowledged():
    received = []
    smartpot = client(received)
    smartpot.client.pending.append((b"topic", b'{"id": "c9", "actuator": "FAN", "action": "ACTIVATE"}'))
    smartpot.poll()
    assert received[0]["id"] == "c9"

    smartpot.acknowledge("c9", True, "Ventilador encendido")
    topic, message, _, qos = smartpot.client.published[-1]
    assert topic == f"smartpot/v1/{CROP}/commands/ack"
    assert json.loads(message)["status"] == "EXECUTED"
    assert qos == 1


def test_tls_without_a_readable_ca_falls_back_to_unverified_encryption(capsys):
    smartpot = client(use_tls=True, ca_file="no-existe.crt")
    assert smartpot.client.options["ssl"] is True
    assert smartpot.client.options["ssl_params"] == {"server_hostname": "mqtt.smartpot.app"}
    assert "sin verificación" in capsys.readouterr().out
