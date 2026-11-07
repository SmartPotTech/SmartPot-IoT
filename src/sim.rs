//! Utilidades del build de simulación (`--features sim`) para Wokwi.
//!
//! Nada de este módulo entra en el firmware de producción.

/// Habilita la interrupción del temporizador 0 de TIMG0 en `INT_ENA_TIMERS`.
///
/// En el ESP32 real ese registro no filtra la interrupción y `esp-hal` solo
/// activa `LEVEL_INT_EN` en la configuración del temporizador. Wokwi sí aplica
/// la máscara de `INT_ENA`, así que sin este bit la alarma de `esp-rtos` nunca
/// llega, el tiempo de Embassy no avanza y el arranque del Wi-Fi se queda
/// esperando. En el silicio la escritura no tiene efecto.
pub fn enable_timer_interrupt_for_simulator() {
    const TIMG0_INT_ENA_TIMERS_REG: *mut u32 = 0x3FF5_F098 as *mut u32;
    // SAFETY: lectura-modificación-escritura de un registro de 32 bits del
    // periférico TIMG0; solo se activa el bit del temporizador 0.
    unsafe {
        let value = core::ptr::read_volatile(TIMG0_INT_ENA_TIMERS_REG);
        core::ptr::write_volatile(TIMG0_INT_ENA_TIMERS_REG, value | 1);
    }
}
