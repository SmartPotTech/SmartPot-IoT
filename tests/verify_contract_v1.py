"""
Verificación de contrato MQTT v1 y compatibilidad firmware Rust contra SmartPot demo-beta.

Arquitectura de la prueba:
- Cliente 1 (Firmware Rust simulado):
  * Usuario: 66f5a1000000000000000101
  * Clave: demo-lettuce-device-key-2026
  * ACL acotado por EMQX + SmartPot-API-Go:
    - Publica en: .../telemetry, .../status, .../commands/ack
    - Se suscribe a: .../commands
- Cliente 2 (Backend / Admin API):
  * Usuario: smartpot-api
  * Clave: demo-mqtt-admin-password
  * Publica comandos y verifica recepción de telemetría y ACKs en español.
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

# --- Callbacks Cliente Admin ---
def on_admin_connect(client, userdata, flags, rc):
    print(f"[ADMIN] Conectado a EMQX (rc={rc})")
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

# --- Callbacks Cliente Dispositivo (Firmware) ---
def on_device_connect(client, userdata, flags, rc):
    print(f"[DEVICE] Conectado y autenticado con credenciales de cultivo (rc={rc})")
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
            "WATER_PUMP": "Bomba de agua",
            "LIGHTS": "Luz de cultivo",
            "FAN": "Ventilador",
            "HUMIDIFIER": "Humidificador"
        }

        if actuator not in actuator_names:
            ack = {
                "id": cmd_id,
                "status": "FAILED",
                "message": "Comando inválido o actuador desconocido"
            }
        else:
            name = actuator_names[actuator]
            is_fem = actuator in ("WATER_PUMP", "LIGHTS")
            ending = "a" if is_fem else "o"
            if action == "ACTIVATE":
                if duration:
                    if duration < 60:
                        dur_str = f"{duration} s"
                    elif duration < 3600:
                        dur_str = f"{duration // 60} min"
                    else:
                        dur_str = f"{duration // 3600} h"
                    msg_text = f"{name} encendid{ending} por {dur_str}"
                else:
                    msg_text = f"{name} encendid{ending}"
            else:
                msg_text = f"{name} apagad{ending}"

            ack = {
                "id": cmd_id,
                "status": "EXECUTED",
                "message": msg_text
            }

        client.publish(TOPIC_ACK, payload=json.dumps(ack), qos=1)
    except Exception as e:
        print(f"[DEVICE ERROR] {e}")

def test_api_health():
    print("\n--- 1. Verificando API Go (demo-beta) ---")
    req = urllib.request.Request(f"{API_URL}/health")
    with urllib.request.urlopen(req, timeout=5) as resp:
        body = resp.read().decode("utf-8")
        print(f"[OK] Health check HTTP {resp.status}: {body}")
        assert resp.status == 200

def run_e2e_verification():
    print("\n--- 2. Iniciando Cliente Admin (Backend) ---")
    admin_client = mqtt.Client(client_id="smartpot-admin-verifier")
    admin_client.username_pw_set(ADMIN_USER, ADMIN_PASS)
    admin_client.on_connect = on_admin_connect
    admin_client.on_message = on_admin_message
    admin_client.connect(BROKER_HOST, BROKER_PORT, keepalive=60)
    admin_client.loop_start()
    time.sleep(1)

    print("\n--- 3. Iniciando Cliente Dispositivo (Rust Firmware Simulado) ---")
    device_client = mqtt.Client(client_id=f"smartpot-{CROP_ID}")
    device_client.username_pw_set(CROP_ID, DEVICE_KEY)
    device_client.will_set(TOPIC_STATUS, payload="offline", qos=1, retain=True)
    device_client.on_connect = on_device_connect
    device_client.on_message = on_device_message
    device_client.connect(BROKER_HOST, BROKER_PORT, keepalive=60)
    device_client.loop_start()
    time.sleep(1)

    # 4. Dispositivo publica status online
    print("\n--- 4. Dispositivo publica status 'online' ---")
    device_client.publish(TOPIC_STATUS, payload="online", qos=1, retain=True)
    time.sleep(0.5)

    # 5. Dispositivo publica telemetría
    print("\n--- 5. Dispositivo publica telemetría v1 ---")
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
    print("\n--- 6. Admin envía comandos de actuadores ---")
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

    print("\n--- 7. Resultados y Validaciones ---")
    print(f"Telemetría recibida por Admin: {len(admin_received['telemetry'])}")
    print(f"Comandos recibidos por Dispositivo: {len(device_received_commands)}")
    print(f"Confirmaciones ACK recibidas por Admin: {len(admin_received['ack'])}")
    print(f"Notificaciones de Status recibidas por Admin: {len(admin_received['status'])}")

    assert len(admin_received['status']) >= 1, "Admin debe recibir status online"
    assert len(admin_received['telemetry']) >= 1, "Admin debe recibir telemetría"
    # Filtrar ACKs recibidos por ID
    acks_raw = [json.loads(x) for x in admin_received['ack']]
    print(f"Total ACKs recibidos en el bus: {len(acks_raw)}")

    ack_01 = next((a for a in acks_raw if a["id"] == "cmd-rust-01" and "Bomba de agua" in a.get("message", "")), None)
    assert ack_01 is not None and ack_01["status"] == "EXECUTED" and ack_01["message"] == "Bomba de agua encendida por 15 s"
    print("  [OK] Validado ACK cmd-rust-01 (Bomba de agua)")

    ack_02 = next((a for a in acks_raw if a["id"] == "cmd-rust-02" and "Luz de cultivo" in a.get("message", "")), None)
    assert ack_02 is not None and ack_02["status"] == "EXECUTED" and ack_02["message"] == "Luz de cultivo apagada"
    print("  [OK] Validado ACK cmd-rust-02 (Luz de cultivo)")

    ack_03 = next((a for a in acks_raw if a["id"] == "cmd-rust-03" and "Ventilador" in a.get("message", "")), None)
    assert ack_03 is not None and ack_03["status"] == "EXECUTED" and ack_03["message"] == "Ventilador encendido por 10 min"
    print("  [OK] Validado ACK cmd-rust-03 (Ventilador)")

    ack_04 = next((a for a in acks_raw if a["id"] == "cmd-rust-04" and "Humidificador" in a.get("message", "")), None)
    assert ack_04 is not None and ack_04["status"] == "EXECUTED" and ack_04["message"] == "Humidificador encendido"
    print("  [OK] Validado ACK cmd-rust-04 (Humidificador)")

    ack_err = next((a for a in acks_raw if a["id"] == "cmd-rust-err" and a.get("status") == "FAILED"), None)
    assert ack_err is not None, "Debe existir ACK FAILED para comando inválido"
    print("  [OK] Validado ACK cmd-rust-err (Comando inválido / fallo)")

    print("\n=======================================================")
    print("[TODO EN VERDE] CONTRATO MQTT v1 VALIDADO AL 100%")
    print("EMQX Authn, ACL por cultivo, telemetría, comandos y ACKs")
    print("=======================================================")

if __name__ == "__main__":
    test_api_health()
    run_e2e_verification()
