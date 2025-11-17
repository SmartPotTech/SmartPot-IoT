from machine import Pin


class Actuator:
    def __init__(self, pin_num):
        self.pin = Pin(pin_num, Pin.OUT)
        self.state = False  # False means OFF, True means ON

    def turn_on(self):
        self.pin.on()  # Activate the actuator
        self.state = True
        print(f"{self.__class__.__name__} turned ON")

    def turn_off(self):
        self.pin.off()  # Deactivate the actuator
        self.state = False
        print(f"{self.__class__.__name__} turned OFF")

    def get_state(self):
        return "ON" if self.state else "OFF"


# Class for Water Pump
class WaterPump(Actuator):
    def __init__(self, pin_num):
        super().__init__(pin_num)


# Class for UV Light
class UVLight(Actuator):
    def __init__(self, pin_num):
        super().__init__(pin_num)
