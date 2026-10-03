from display import ICONS, OFFLINE, ONLINE, SPINNER, LCDDisplay

READINGS = {"temperature": 23.5, "humidity": 61, "brightness": 820, "ph": 6.1, "tds": 790, "soilMoisture": 64}


def test_spinner_uses_glyphs_from_the_lcd_rom():
    lcd = LCDDisplay(scl_pin=16, sda_pin=17)
    seen = set()
    for _ in SPINNER:
        lcd.show_spinner()
        seen.add(ord(lcd.lcd.rows[3][19]))
    assert seen == {0xA1, 0xA5, 0xDB}


def test_link_icons_are_loaded_into_the_lcd():
    lcd = LCDDisplay(scl_pin=16, sda_pin=17)
    assert lcd.lcd.glyphs == {0: list(ICONS[0]), 1: list(ICONS[1])}


def test_readings_show_the_actuators_and_whether_the_device_is_online():
    lcd = LCDDisplay(scl_pin=16, sda_pin=17)
    lcd.show_readings(READINGS, {"WATER_PUMP": True, "UV_LIGHT": False, "FAN": True}, online=True)
    assert lcd.lcd.rows[0] == "T 23.5C H  61%     " + ONLINE
    assert lcd.lcd.rows[3].startswith("TDS  790ppm [B-V]")

    lcd.show_readings({}, {}, online=False)
    assert lcd.lcd.rows[0] == "T   --C H  --%     " + OFFLINE
