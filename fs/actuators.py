from machine import Pin


def readable(seconds):
    """La duración como la lee una persona: 15 s, 10 min, 2 h."""
    if seconds % 3600 == 0:
        return "{} h".format(seconds // 3600)
    if seconds % 60 == 0:
        return "{} min".format(seconds // 60)
    return "{} s".format(seconds)


class Actuator:
    """Salida digital (relé o LED en la simulación) con apagado automático opcional."""

    def __init__(self, name, pin_num, feminine=False):
        self.name = name
        # Para concordar el mensaje: «Bomba de agua encendida», «Ventilador encendido».
        self.ending = "a" if feminine else "o"
        self.pin = Pin(pin_num, Pin.OUT)
        self.active = False
        self.until = None
        self.pin.off()

    def turn_on(self, duration=None, now=0):
        self.pin.on()
        self.active = True
        self.until = now + duration if duration else None

    def turn_off(self):
        self.pin.off()
        self.active = False
        self.until = None

    def tick(self, now):
        if self.active and self.until is not None and now >= self.until:
            self.turn_off()
            print(self.name, "apagad" + self.ending, "por tiempo")


class ActuatorBank:
    """Ejecuta los comandos de la API sobre los actuadores conectados."""

    def __init__(self, actuators):
        self.actuators = actuators

    def execute(self, command, now):
        actuator = self.actuators.get(command["actuator"])
        if actuator is None:
            return False, "Este dispositivo no tiene " + command["actuator"]
        if command["action"] == "ACTIVATE":
            duration = command.get("durationSeconds")
            actuator.turn_on(duration, now)
            state = " encendid" + actuator.ending
            return True, actuator.name + (state + " por " + readable(duration) if duration else state)
        if command["action"] == "DEACTIVATE":
            actuator.turn_off()
            return True, actuator.name + " apagad" + actuator.ending
        return False, "Acción desconocida: " + command["action"]

    def tick(self, now):
        for actuator in self.actuators.values():
            actuator.tick(now)

    def states(self):
        return {name: actuator.active for name, actuator in self.actuators.items()}
