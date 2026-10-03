from actuators import Actuator, ActuatorBank
from conftest import FakeADC, FakeDHT22
from sensors import AtmosphereSensor, LightSensor, PHSensor, SoilMoistureSensor, TDSSensor
from utils import clock_ready, format_row, print_table, timestamp


def bank():
    return ActuatorBank({"WATER_PUMP": Actuator("Bomba de agua", 19, feminine=True), "FAN": Actuator("Ventilador", 5)})


def test_timed_activation_turns_off_by_itself():
    actuators = bank()
    executed, message = actuators.execute({"actuator": "WATER_PUMP", "action": "ACTIVATE", "durationSeconds": 15}, 100)
    pump = actuators.actuators["WATER_PUMP"]
    assert executed and message == "Bomba de agua encendida por 15 s"
    assert pump.pin.level == 1
    actuators.tick(110)
    assert pump.active
    actuators.tick(115)
    assert not pump.active and pump.pin.level == 0


def test_acknowledgements_read_durations_like_a_person():
    actuators = bank()
    fan = actuators.execute({"actuator": "FAN", "action": "ACTIVATE", "durationSeconds": 600}, 0)
    pump = actuators.execute({"actuator": "WATER_PUMP", "action": "ACTIVATE", "durationSeconds": 7200}, 0)
    assert fan == (True, "Ventilador encendido por 10 min")
    assert pump == (True, "Bomba de agua encendida por 2 h")


def test_activation_without_duration_stays_on_until_deactivated():
    actuators = bank()
    actuators.execute({"actuator": "FAN", "action": "ACTIVATE", "durationSeconds": None}, 0)
    actuators.tick(10_000)
    assert actuators.states()["FAN"] is True
    assert actuators.execute({"actuator": "FAN", "action": "DEACTIVATE"}, 1) == (True, "Ventilador apagado")
    assert actuators.states()["FAN"] is False


def test_unknown_actuators_and_actions_fail():
    actuators = bank()
    missing = actuators.execute({"actuator": "HUMIDIFIER", "action": "ACTIVATE"}, 0)
    assert missing == (False, "Este dispositivo no tiene HUMIDIFIER")
    assert actuators.execute({"actuator": "FAN", "action": "BLINK"}, 0)[0] is False


def test_analog_sensors_use_the_device_scale():
    FakeADC.readings = {34: 65535, 35: 32768, 32: 0, 33: 16384}
    assert LightSensor(34).read() == 2000
    assert round(PHSensor(35).read(), 1) == 7.0
    assert TDSSensor(32).read() == 0
    assert round(SoilMoistureSensor(33).read()) == 25


def test_sensor_failures_return_none():
    FakeADC.readings = {34: None}
    assert LightSensor(34).read() is None
    FakeDHT22.fail = True
    assert AtmosphereSensor(15).read() == {"temperature": None, "humidity": None}
    FakeDHT22.fail = False
    assert AtmosphereSensor(15).read() == {"temperature": 23.5, "humidity": 61.0}


def test_console_table_rows():
    assert format_row("pH", 6.1234, "") == "| pH             | 6.12           |"
    assert format_row("Luz", None, "lux") == "| Luz            | --             |"


def test_console_table_tells_when_the_reading_was_taken(capsys):
    print_table({"ph": 6.1}, (2026, 10, 2, 19, 40, 5, 4, 275))
    lines = capsys.readouterr().out.splitlines()
    assert lines[1] == "| Lectura 2026-10-02 19:40:05 UTC |"
    assert {len(line) for line in lines} == {35}


def test_the_clock_is_ready_only_after_syncing():
    assert not clock_ready((2000, 1, 1, 0, 0, 12, 5, 1))
    assert timestamp((2000, 1, 1, 0, 0, 12, 5, 1)) == "hora sin sincronizar"
    assert clock_ready((2026, 10, 2, 19, 40, 5, 4, 275))
