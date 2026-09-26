from actuators import Actuator, ActuatorBank
from conftest import FakeADC, FakeDHT22
from sensors import AtmosphereSensor, LightSensor, PHSensor, SoilMoistureSensor, TDSSensor
from utils import format_row


def bank():
    return ActuatorBank({"WATER_PUMP": Actuator("Bomba", 19), "FAN": Actuator("Ventilador", 5)})


def test_timed_activation_turns_off_by_itself():
    actuators = bank()
    executed, message = actuators.execute({"actuator": "WATER_PUMP", "action": "ACTIVATE", "durationSeconds": 15}, 100)
    pump = actuators.actuators["WATER_PUMP"]
    assert executed and message == "Bomba encendido 15 s"
    assert pump.pin.level == 1
    actuators.tick(110)
    assert pump.active
    actuators.tick(115)
    assert not pump.active and pump.pin.level == 0


def test_activation_without_duration_stays_on_until_deactivated():
    actuators = bank()
    actuators.execute({"actuator": "FAN", "action": "ACTIVATE", "durationSeconds": None}, 0)
    actuators.tick(10_000)
    assert actuators.states()["FAN"] is True
    assert actuators.execute({"actuator": "FAN", "action": "DEACTIVATE"}, 1) == (True, "Ventilador apagado")
    assert actuators.states()["FAN"] is False


def test_unknown_actuators_and_actions_fail():
    actuators = bank()
    assert actuators.execute({"actuator": "HUMIDIFIER", "action": "ACTIVATE"}, 0) == (False,
                                                                                     "La maceta no tiene HUMIDIFIER")
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
