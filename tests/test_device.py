from actuators import Actuator, ActuatorBank
from device import RETRY_SECONDS, WIFI_RETRY_SECONDS, Device


class FakeLink:
    def __init__(self, wifi=True, clock=True, sync=True):
        self.wifi, self.clock, self.sync = wifi, clock, sync
        self.wifi_attempts = 0
        self.syncs = 0

    def wifi_up(self):
        return self.wifi

    def reconnect_wifi(self):
        self.wifi_attempts += 1

    def clock_ready(self):
        return self.clock

    def sync_time(self):
        self.syncs += 1
        self.clock = self.sync
        return self.sync


class FakeClient:
    def __init__(self, fail_connect=False):
        self.fail_connect = fail_connect
        self.poll_error = None
        self.connects = 0
        self.closed = 0
        self.published = []

    def connect(self):
        self.connects += 1
        if self.fail_connect:
            raise OSError("broker caído")

    def poll(self):
        if self.poll_error:
            raise self.poll_error

    def publish_telemetry(self, readings):
        self.published.append(readings)

    def close(self):
        self.closed += 1


class FakeScreen:
    def __init__(self):
        self.shown = []

    def show_readings(self, readings, states, online):
        self.shown.append((readings, states, online))


def device(client=None, link=None, **options):
    bank = ActuatorBank({"WATER_PUMP": Actuator("Bomba de agua", 19, feminine=True)})
    return Device(lambda: {"ph": 6.1}, bank, FakeScreen(), client, link or FakeLink(), 30, **options)


def test_measures_and_shows_readings_without_a_broker():
    unit = device(client=None)
    unit.step(0)
    assert unit.lcd.shown == [({"ph": 6.1}, {"WATER_PUMP": False}, False)]
    unit.step(10)
    unit.step(30)
    assert len(unit.lcd.shown) == 2


def test_a_timed_actuator_turns_off_while_offline():
    client = FakeClient()
    unit = device(client, FakeLink(wifi=False))
    unit.bank.execute({"actuator": "WATER_PUMP", "action": "ACTIVATE", "durationSeconds": 15}, 0)
    unit.step(0)
    assert unit.bank.states()["WATER_PUMP"]
    unit.step(15)
    assert not unit.bank.states()["WATER_PUMP"]
    assert client.connects == 0


def test_connects_and_publishes_once_wifi_and_clock_are_ready():
    client = FakeClient()
    unit = device(client)
    unit.step(0)
    assert unit.online and client.connects == 1
    assert client.published == [{"ph": 6.1}]
    assert unit.lcd.shown[-1][2] is True


def test_tls_waits_for_the_clock_before_connecting():
    link = FakeLink(clock=False, sync=False)
    client = FakeClient()
    unit = device(client, link)
    unit.step(0)
    assert client.connects == 0 and link.syncs == 1
    link.sync = True
    unit.step(RETRY_SECONDS - 1)
    assert link.syncs == 1
    unit.step(RETRY_SECONDS)
    assert unit.online and client.connects == 1


def test_plain_mqtt_does_not_need_the_clock():
    link = FakeLink(clock=False, sync=False)
    unit = device(FakeClient(), link, secure=False)
    unit.step(0)
    assert unit.online and link.syncs == 0


def test_a_lost_connection_is_retried_after_10_seconds():
    client = FakeClient()
    unit = device(client)
    unit.step(0)
    client.poll_error = OSError("sin red")
    unit.step(1)
    assert not unit.online and client.closed == 1
    client.poll_error = None
    unit.step(RETRY_SECONDS)
    assert client.connects == 1
    unit.step(1 + RETRY_SECONDS)
    assert unit.online and client.connects == 2


def test_a_failed_connection_frees_the_socket():
    client = FakeClient(fail_connect=True)
    unit = device(client)
    unit.step(0)
    assert not unit.online and client.closed == 1


def test_wifi_is_retried_every_30_seconds():
    link = FakeLink(wifi=False)
    unit = device(FakeClient(), link)
    unit.step(0)
    unit.step(RETRY_SECONDS)
    assert link.wifi_attempts == 1
    unit.step(WIFI_RETRY_SECONDS)
    assert link.wifi_attempts == 2
