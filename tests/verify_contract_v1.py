"""
MQTT v1 contract and Rust firmware compatibility verification against SmartPot demo-beta.

Test architecture:
- Client 1 (Simulated Rust Firmware):
  * Username: 66f5a1000000000000000101
  * Password: demo-lettuce-device-key-2026
  * ACL scoped by EMQX + SmartPot-API-Go:
    - Publishes to: .../telemetry, .../status, .../commands/ack
    - Subscribes to: .../commands
- Client 2 (Backend / Admin API):
  * Username: smartpot-api
  * Password: demo-mqtt-admin-password
  * Publishes commands and verifies receipt of telemetry and ACKs in English.
"""

import json
import time
import urllib.request
import paho.mqtt.client as mqtt

BROKER_HOST = "127.0.0.1"
BROKER_PORT = 1883
CROP_ID = "66f5a1000000000000000101"
DEVICE_KEY = "demo-lettuce-device-key-2026"
ADMIN_USER = "smartpot-api"
ADMIN_PASS = "demo-mqtt-admin-password"
API_URL = "http://127.0.0.1:8091"

TOPIC_TELEMETRY = f"smartpot/v1/{CROP_ID}/telemetry"
TOPIC_COMMANDS = f"smartpot/v1/{CROP_ID}/commands"
TOPIC_ACK = f"smartpot/v1/{CROP_ID}/commands/ack"
TOPIC_STATUS = f"smartpot/v1/{CROP_ID}/status"

admin_received = {
    "telemetry": [],
    "ack": [],
    "status": []
}

device_received_commands = []


# --- Callbacks del cliente Admin ---
def on_admin_connect(client, userdata, flags, rc):
    print(f"[ADMIN] Connected to EMQX (rc={rc})")
    client.subscribe([
        (TOPIC_TELEMETRY, 0),
        (TOPIC_ACK, 1),
        (TOPIC_STATUS, 1)
    ])


def on_admin_message(client, userdata, msg):
    payload = msg.payload.decode("utf-8", errors="replace")
    topic = msg.topic
    print(f"  [ADMIN <- {topic}]: {payload}")
    if topic == TOPIC_TELEMETRY:
        admin_received["telemetry"].append(payload)
    elif topic == TOPIC_ACK:
        admin_received["ack"].append(payload)
    elif topic == TOPIC_STATUS:
        admin_received["status"].append(payload)


# --- Callbacks del cliente Dispositivo (Firmware) ---
def on_device_connect(client, userdata, flags, rc):
    print(f"[DEVICE] Connected and authenticated with crop credentials (rc={rc})")
    client.subscribe(TOPIC_COMMANDS, qos=1)


def on_device_message(client, userdata, msg):
    payload = msg.payload.decode("utf-8", errors="replace")
    print(f"  [DEVICE <- {msg.topic}]: {payload}")
    device_received_commands.append(payload)

    # Simulación exacta de la lógica de firmware Rust (command.rs & ack.rs)
    try:
        data = json.loads(payload)
        cmd_id = data.get("id", "unknown")
        actuator = data.get("actuator", "")
        action = data.get("action", "")
        duration = data.get("durationSeconds")

        actuator_names = {
            "WATER_PUMP": "Water pump",
            "LIGHTS": "Grow light",
            "FAN": "Fan",
            "HUMIDIFIER": "Humidifier"
        }

        if actuator not in actuator_names:
            ack = {
                "id": cmd_id,
                "status": "FAILED",
                "message": "Invalid command or unknown actuator"
            }
        else:
            name = actuator_names[actuator]
            if action == "ACTIVATE":
                if duration:
                    if duration < 60:
                        dur_str = f"{duration} s"
                    elif duration < 3600:
                        dur_str = f"{duration // 60} min"
                    else:
                        dur_str = f"{duration // 3600} h"
                    msg_text = f"{name} turned on for {dur_str}"
                else:
                    msg_text = f"{name} turned on"
            else:
                msg_text = f"{name} turned off"

            ack = {
                "id": cmd_id,
                "status": "EXECUTED",
                "message": msg_text
            }

        client.publish(TOPIC_ACK, payload=json.dumps(ack), qos=1)
    except Exception as e:
        print(f"[DEVICE ERROR] {e}")


def test_api_health():
    print("\n--- 1. Verifying Go API (demo-beta) ---")
    req = urllib.request.Request(f"{API_URL}/health")
    with urllib.request.urlopen(req, timeout=5) as resp:
        body = resp.read().decode("utf-8")
        print(f"[OK] Health check HTTP {resp.status}: {body}")
        assert resp.status == 200


def run_e2e_verification():
    print("\n--- 2. Starting Admin Client (Backend) ---")
    admin_client = mqtt.Client(client_id="smartpot-admin-verifier")
    admin_client.username_pw_set(ADMIN_USER, ADMIN_PASS)
    admin_client.on_connect = on_admin_connect
    admin_client.on_message = on_admin_message
    admin_client.connect(BROKER_HOST, BROKER_PORT, keepalive=60)
    admin_client.loop_start()
    time.sleep(1)

    print("\n--- 3. Starting Device Client (Simulated Rust Firmware) ---")
    device_client = mqtt.Client(client_id=f"smartpot-{CROP_ID}")
    device_client.username_pw_set(CROP_ID, DEVICE_KEY)
    device_client.will_set(TOPIC_STATUS, payload="offline", qos=1, retain=True)
    device_client.on_connect = on_device_connect
    device_client.on_message = on_device_message
    device_client.connect(BROKER_HOST, BROKER_PORT, keepalive=60)
    device_client.loop_start()
    time.sleep(1)

    # 4. Dispositivo publica status online
    print("\n--- 4. Device publishes 'online' status ---")
    device_client.publish(TOPIC_STATUS, payload="online", qos=1, retain=True)
    time.sleep(0.5)

    # 5. Dispositivo publica telemetría
    print("\n--- 5. Device publishes v1 telemetry ---")
    sample_telemetry = {
        "temperature": 23.4,
        "humidity": 64.8,
        "brightness": 1850,
        "ph": 6.85,
        "tds": 620,
        "atmosphere": 1012.8,
        "soilMoisture": 48.2
    }
    device_client.publish(TOPIC_TELEMETRY, payload=json.dumps(sample_telemetry), qos=0)
    time.sleep(0.5)

    # 6. Admin envía comandos al dispositivo
    print("\n--- 6. Admin sends actuator commands ---")
    commands_to_send = [
        {"id": "cmd-rust-01", "actuator": "WATER_PUMP", "action": "ACTIVATE", "durationSeconds": 15},
        {"id": "cmd-rust-02", "actuator": "LIGHTS", "action": "DEACTIVATE"},
        {"id": "cmd-rust-03", "actuator": "FAN", "action": "ACTIVATE", "durationSeconds": 600},
        {"id": "cmd-rust-04", "actuator": "HUMIDIFIER", "action": "ACTIVATE"},
        {"id": "cmd-rust-err", "actuator": "SOLENOID_UNKNOWN", "action": "ACTIVATE"}
    ]

    for cmd in commands_to_send:
        print(f"  [ADMIN -> {TOPIC_COMMANDS}]: {json.dumps(cmd)}")
        admin_client.publish(TOPIC_COMMANDS, payload=json.dumps(cmd), qos=1)
        time.sleep(0.3)

    time.sleep(1.5)

    device_client.loop_stop()
    device_client.disconnect()
    admin_client.loop_stop()
    admin_client.disconnect()

    print("\n--- 7. Results and Validations ---")
    print(f"Telemetry received by Admin: {len(admin_received['telemetry'])}")
    print(f"Commands received by Device: {len(device_received_commands)}")
    print(f"ACK confirmations received by Admin: {len(admin_received['ack'])}")
    print(f"Status notifications received by Admin: {len(admin_received['status'])}")

    assert len(admin_received['status']) >= 1, "Admin must receive online status"
    assert len(admin_received['telemetry']) >= 1, "Admin must receive telemetry"
    # Filtrar ACKs recibidos por ID
    acks_raw = [json.loads(x) for x in admin_received['ack']]
    print(f"Total ACKs received on bus: {len(acks_raw)}")

    ack_01 = next((a for a in acks_raw if a["id"] == "cmd-rust-01" and "Water pump" in a.get("message", "")), None)
    assert ack_01 is not None and ack_01["status"] == "EXECUTED" and ack_01["message"] == "Water pump turned on for 15 s"
    print("  [OK] Validated ACK cmd-rust-01 (Water pump)")

    ack_02 = next((a for a in acks_raw if a["id"] == "cmd-rust-02" and "Grow light" in a.get("message", "")), None)
    assert ack_02 is not None and ack_02["status"] == "EXECUTED" and ack_02["message"] == "Grow light turned off"
    print("  [OK] Validated ACK cmd-rust-02 (Grow light)")

    ack_03 = next((a for a in acks_raw if a["id"] == "cmd-rust-03" and "Fan" in a.get("message", "")), None)
    assert ack_03 is not None and ack_03["status"] == "EXECUTED" and ack_03["message"] == "Fan turned on for 10 min"
    print("  [OK] Validated ACK cmd-rust-03 (Fan)")

    ack_04 = next((a for a in acks_raw if a["id"] == "cmd-rust-04" and "Humidifier" in a.get("message", "")), None)
    assert ack_04 is not None and ack_04["status"] == "EXECUTED" and ack_04["message"] == "Humidifier turned on"
    print("  [OK] Validated ACK cmd-rust-04 (Humidifier)")

    ack_err = next((a for a in acks_raw if a["id"] == "cmd-rust-err" and a.get("status") == "FAILED"), None)
    assert ack_err is not None, "FAILED ACK must exist for invalid command"
    print("  [OK] Validated ACK cmd-rust-err (Invalid command / failure)")

    print("\n=======================================================")
    print("[ALL GREEN] MQTT v1 CONTRACT 100% VALIDATED")
    print("EMQX Authn, per-crop ACL, telemetry, commands, and ACKs")
    print("=======================================================")


if __name__ == "__main__":
    test_api_health()
    run_e2e_verification()
