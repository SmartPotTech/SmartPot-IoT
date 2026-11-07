//! Utilidades de compatibilidad con el simulador Wokwi para ESP32.
//!
//! Se compila siempre para que el binario por defecto (`cargo build`) funcione
//! directamente en Wokwi sin requerir flags adicionales y sin afectar al
//! comportamiento en silicio real.

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

/// Ganchos de prueba del build de simulación (`--features sim`).
///
/// Las escenas de Wokwi escriben por la consola serie líneas JSON:
/// - un comando del contrato v1 (`{"id":..,"actuator":..,"action":..}`), que
///   entra en el mismo canal que los comandos MQTT;
/// - `{"sim":"panic"}`, `{"sim":"hang"}` (bloquea el núcleo 0),
///   `{"sim":"hang-core1"}` y `{"sim":"leak"}` (pierde heap a propósito para
///   comprobar que la prueba de resistencia detecta fugas).
#[cfg(feature = "sim")]
pub mod hooks {
    use core::sync::atomic::{AtomicBool, Ordering};

    use embassy_sync::pubsub::DynPublisher;
    use esp_hal::Async;
    use esp_hal::uart::UartRx;
    use heapless::String;

    use crate::events::Command;
    use crate::protocol::command::extract_str_field;
    use crate::protocol::parse_command;

    static HANG_CORE1: AtomicBool = AtomicBool::new(false);
    static LEAK: AtomicBool = AtomicBool::new(false);

    /// La tarea de comandos del núcleo 1 se bloquea si una escena lo pidió.
    pub fn core1_should_hang() -> bool {
        HANG_CORE1.load(Ordering::Relaxed)
    }

    /// Pierde 2 KiB de heap por llamada mientras la fuga simulada esté activa.
    pub fn leak_if_requested() {
        if LEAK.load(Ordering::Relaxed) {
            let block = alloc::vec![0x5Au8; 2048].into_boxed_slice();
            let _ = alloc::boxed::Box::leak(block);
        }
    }

    fn handle_line(line: &str, commands: &DynPublisher<'static, Command>) {
        if let Ok(action) = extract_str_field(line, "sim") {
            marker!("SMARTPOT-SIM {}", action.as_str());
            match action.as_str() {
                "panic" => panic!("simulated panic"),
                "hang" => loop {
                    core::hint::spin_loop();
                },
                "hang-core1" => HANG_CORE1.store(true, Ordering::Relaxed),
                "leak" => LEAK.store(true, Ordering::Relaxed),
                _ => warn!("[SIM] Unknown simulation hook: {}", action.as_str()),
            }
            return;
        }

        match parse_command(line) {
            Ok(command) => commands.publish_immediate(command),
            Err(reason) => {
                let id = extract_str_field(line, "id").unwrap_or_default();
                marker!("SMARTPOT-ACK {} FAILED {}", id.as_str(), reason.message());
            }
        }
    }

    /// Lee líneas de la consola serie y las entrega como comandos o ganchos.
    #[embassy_executor::task]
    pub async fn serial_injector(
        mut rx: UartRx<'static, Async>,
        commands: DynPublisher<'static, Command>,
    ) {
        let mut line: String<256> = String::new();
        let mut chunk = [0u8; 64];
        marker!("Serial command injector ready");
        loop {
            let Ok(count) = rx.read_async(&mut chunk).await else {
                line.clear();
                continue;
            };
            for &byte in &chunk[..count] {
                match byte {
                    b'\n' | b'\r' => {
                        if !line.is_empty() {
                            handle_line(line.as_str(), &commands);
                            line.clear();
                        }
                    }
                    byte if line.push(byte as char).is_err() => line.clear(),
                    _ => {}
                }
            }
        }
    }
}
