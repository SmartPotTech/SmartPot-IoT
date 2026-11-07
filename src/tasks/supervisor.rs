//! Supervisor del watchdog RTC y métricas de memoria.
//!
//! `esp_hal::init` desactiva todos los watchdogs. Esta tarea vuelve a activar
//! el RWDT y solo lo alimenta mientras todas las tareas vigiladas siguen
//! latiendo. Si una tarea se detiene, apaga los actuadores y reinicia el chip;
//! si se bloquea el propio núcleo 0, el RWDT reinicia el sistema sin ayuda.
//! También publica el uso del heap y la marca de agua de la pila del núcleo 1.

use embassy_executor::task;
use embassy_time::{Duration, Instant, Ticker};
use esp_hal::rtc_cntl::{Rtc, RwdtStage, RwdtStageAction};

use crate::config::parse_u64_or;
use crate::health::{self, BEAT_NAMES, Liveness};
use crate::safety;

/// Tiempo sin alimentar el RWDT tras el que se reinicia el sistema.
const WATCHDOG_TIMEOUT_SECS: u64 = match option_env!("WATCHDOG_TIMEOUT_SECS") {
    Some(v) => parse_u64_or(v, 30),
    None => 30,
};

/// Tiempo máximo sin latido antes de considerar detenida una tarea.
const LIVENESS_TIMEOUT_SECS: u64 = match option_env!("LIVENESS_TIMEOUT_SECS") {
    Some(v) => parse_u64_or(v, 120),
    None => 120,
};

/// Periodo del informe de memoria por la consola serie.
const HEAP_LOG_SECS: u64 = match option_env!("HEAP_LOG_SECS") {
    Some(v) => parse_u64_or(v, 60),
    None => 60,
};

/// Umbral opcional de heap libre (`HEAP_MIN_FREE` al compilar): por debajo se
/// imprime `SMARTPOT-HEAP-LOW`, que los escenarios de Wokwi usan como fallo.
const HEAP_LOW_THRESHOLD: Option<u64> = match option_env!("HEAP_MIN_FREE") {
    Some(v) => Some(parse_u64_or(v, 0)),
    None => None,
};

const CHECK_PERIOD: Duration = Duration::from_secs(5);

#[task]
pub async fn supervisor_task(mut rtc: Rtc<'static>) {
    rtc.rwdt
        .set_stage_action(RwdtStage::Stage0, RwdtStageAction::ResetSystem);
    rtc.rwdt.set_timeout(
        RwdtStage::Stage0,
        esp_hal::time::Duration::from_secs(WATCHDOG_TIMEOUT_SECS),
    );
    rtc.rwdt.enable();
    rtc.rwdt.feed();
    info!(
        "[WDT] RTC watchdog armed: {} s, task liveness {} s",
        WATCHDOG_TIMEOUT_SECS, LIVENESS_TIMEOUT_SECS
    );

    let mut liveness = Liveness::new();
    let mut ticker = Ticker::every(CHECK_PERIOD);
    let mut next_report = Instant::now();

    loop {
        ticker.next().await;
        let now = Instant::now();
        #[cfg(feature = "sim")]
        crate::sim::hooks::leak_if_requested();

        match liveness.check(
            health::counts(),
            now.as_millis(),
            LIVENESS_TIMEOUT_SECS * 1000,
        ) {
            None => rtc.rwdt.feed(),
            Some(index) => {
                safety::force_actuators_off();
                marker!("SMARTPOT-FATAL watchdog task={} stalled", BEAT_NAMES[index]);
                // Deja salir el marcador por la UART antes de reiniciar.
                esp_hal::delay::Delay::new().delay_millis(50);
                esp_hal::system::software_reset();
            }
        }

        if now >= next_report {
            next_report = now + Duration::from_secs(HEAP_LOG_SECS);
            report_memory();
        }
    }
}

/// Imprime el uso del heap y de la pila del núcleo 1.
fn report_memory() {
    let stats = esp_alloc::HEAP.stats();
    let free = stats.size.saturating_sub(stats.current_usage);
    let min_free = stats.size.saturating_sub(stats.max_usage);
    let stack_unused = health::core1_stack_unused_bytes().unwrap_or(0);
    marker!(
        "SMARTPOT-HEAP free={} used={} min_free={} size={} core1_stack_unused={}",
        free,
        stats.current_usage,
        min_free,
        stats.size,
        stack_unused
    );
    if let Some(threshold) = HEAP_LOW_THRESHOLD
        && (free as u64) < threshold
    {
        marker!("SMARTPOT-HEAP-LOW free={} threshold={}", free, threshold);
    }
}
