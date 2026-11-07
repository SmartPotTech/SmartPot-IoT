//! Política de seguridad de los actuadores.
//!
//! La bomba de agua y el humidificador pueden inundar el cultivo si quedan
//! encendidos sin supervisión, así que nunca se encienden sin límite: un
//! ACTIVATE sin duración usa el tope de la API (3600 s) y ambos se apagan si
//! el broker deja de responder durante `OFFLINE_CUTOFF_SECONDS`. La luz UV y el
//! ventilador sí admiten encendido indefinido. El módulo es puro para poder
//! probarlo en el host; solo `force_actuators_off` toca registros del ESP32.

use crate::protocol::Actuators;

/// Duración mínima aceptada por la API para un ACTIVATE temporizado.
pub const MIN_DURATION_SECONDS: u32 = 1;

/// Duración máxima aceptada por la API (`SmartPot-API`, `validateDuration`).
pub const MAX_DURATION_SECONDS: u32 = 3600;

/// Segundos sin broker tras los que se apagan los actuadores críticos.
/// En la simulación se acorta con `OFFLINE_CUTOFF_SECS` al compilar.
pub const OFFLINE_CUTOFF_SECONDS: u64 = match option_env!("OFFLINE_CUTOFF_SECS") {
    Some(value) => crate::config::parse_u64_or(value, 300),
    None => 300,
};

/// Módulos de relé activos en bajo (`ACTUATOR_ACTIVE_LOW=1` al compilar): la
/// salida en alto los mantiene apagados. Por defecto, alto = encendido.
pub const ACTIVE_LOW: bool = match option_env!("ACTUATOR_ACTIVE_LOW") {
    Some(v) => crate::config::parse_u64_or(v, 0) == 1,
    None => false,
};

/// Nivel lógico que enciende un actuador.
pub const fn on_level_is_high() -> bool {
    !ACTIVE_LOW
}

/// GPIO de cada actuador; deben coincidir con los pines de `main.rs`.
///
/// GPIO5 es un pin de *strapping*: emite una señal PWM durante el arranque y
/// puede hacer vibrar el relé del humidificador. En hardware conviene una
/// resistencia de *pull-down* (o una red RC) en la entrada del driver del relé.
pub use crate::board::{FAN_GPIO, HUMIDIFIER_GPIO, UV_LIGHT_GPIO, WATER_PUMP_GPIO};

/// Máscara del registro `GPIO_OUT` (GPIO0..31) con todas las salidas de actuador.
pub const ACTUATOR_GPIO_MASK: u32 = (1 << WATER_PUMP_GPIO)
    | (1 << UV_LIGHT_GPIO)
    | (1 << FAN_GPIO)
    | match HUMIDIFIER_GPIO {
        Some(pin) => 1 << pin,
        None => 0,
    };

/// Indica si el actuador tiene riesgo físico al quedar encendido sin control.
pub const fn is_critical(actuator: Actuators) -> bool {
    matches!(actuator, Actuators::WaterPump | Actuators::Humidifier)
}

/// Indica si una duración explícita está dentro del rango de la API.
pub const fn is_valid_duration(seconds: u32) -> bool {
    seconds >= MIN_DURATION_SECONDS && seconds <= MAX_DURATION_SECONDS
}

/// Duración efectiva de un ACTIVATE: los actuadores críticos sin duración
/// reciben el tope; el resto conserva el encendido indefinido.
pub const fn effective_duration(actuator: Actuators, requested: Option<u32>) -> Option<u32> {
    match requested {
        Some(seconds) => Some(seconds),
        None if is_critical(actuator) => Some(MAX_DURATION_SECONDS),
        None => None,
    }
}

/// Decide si el failsafe debe apagar un actuador crítico encendido.
///
/// `offline_for_ms` es el tiempo sin broker y `on_for_ms` el tiempo que lleva
/// encendido; se cuenta desde el más reciente de los dos, para que una orden
/// recibida justo antes del corte disponga del plazo completo.
pub const fn failsafe_due(
    actuator: Actuators,
    offline_for_ms: Option<u64>,
    on_for_ms: u64,
) -> bool {
    if !is_critical(actuator) {
        return false;
    }
    match offline_for_ms {
        Some(offline) => {
            let elapsed = if offline < on_for_ms {
                offline
            } else {
                on_for_ms
            };
            elapsed >= OFFLINE_CUTOFF_SECONDS * 1000
        }
        None => false,
    }
}

/// Apaga todas las salidas de actuador escribiendo directamente en
/// `GPIO_OUT_W1TC_REG` (o en `W1TS` con relés activos en bajo). Es segura en el
/// manejador de pánico: no usa el heap, no toma cerrojos y no depende de que
/// los drivers de `esp-hal` sigan vivos.
#[cfg(target_arch = "xtensa")]
pub fn force_actuators_off() {
    const GPIO_OUT_W1TS_REG: *mut u32 = 0x3FF4_4008 as *mut u32;
    const GPIO_OUT_W1TC_REG: *mut u32 = 0x3FF4_400C as *mut u32;
    let register = if ACTIVE_LOW {
        GPIO_OUT_W1TS_REG
    } else {
        GPIO_OUT_W1TC_REG
    };
    // SAFETY: escritura de 32 bits alineada a un registro de solo escritura del
    // periférico GPIO del ESP32; solo afecta a los bits de la máscara.
    unsafe { core::ptr::write_volatile(register, ACTUATOR_GPIO_MASK) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_covers_every_actuator_pin() {
        assert_eq!(
            ACTUATOR_GPIO_MASK.count_ones() as usize,
            crate::board::ACTUATOR_COUNT
        );
        assert_ne!(ACTUATOR_GPIO_MASK & (1 << WATER_PUMP_GPIO), 0);
    }
}
