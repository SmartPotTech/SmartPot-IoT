from machine import Pin


class Actuator:
    """Salida digital (relé o LED en la simulación) con apagado automático opcional."""

    def __init__(self, name, pin_num):
        self.name = name
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
            print(self.name, "apagado por tiempo")


class ActuatorBank:
    """Ejecuta los comandos de la API sobre los actuadores conectados."""

    def __init__(self, actuators):
        self.actuators = actuators

    def execute(self, command, now):
        actuator = self.actuators.get(command["actuator"])
        if actuator is None:
            return False, "La maceta no tiene " + command["actuator"]
        if command["action"] == "ACTIVATE":
            duration = command.get("durationSeconds")
            actuator.turn_on(duration, now)
            return True, actuator.name + (" encendido {} s".format(duration) if duration else " encendido")
        if command["action"] == "DEACTIVATE":
            actuator.turn_off()
            return True, actuator.name + " apagado"
        return False, "Acción desconocida: " + command["action"]

    def tick(self, now):
        for actuator in self.actuators.values():
            actuator.tick(now)

    def states(self):
        return {name: actuator.active for name, actuator in self.actuators.items()}
