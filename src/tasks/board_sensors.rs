//! Sensores de la placa compartida con el firmware MicroPython.
//!
//! DHT22 (temperatura y humedad) en GPIO15 y cuatro potenciómetros: luz
//! (0..2000 lx) en GPIO34, pH (0..14) en GPIO35, TDS (0..3000 ppm) en GPIO32 y
//! humedad de suelo (0..100 %) en GPIO33, con las mismas escalas lineales que
//! MicroPython. Cada canal promedia 16 lecturas del ADC y no lleva filtro
//! temporal: con potenciómetros o módulos analógicos simples la mediana y el
//! Kalman solo añaden retraso, y así cada lectura coincide con la de
//! MicroPython. El prototipo físico (`board-physical`) sí filtra sus sondas.

use embassy_executor::task;
use embassy_sync::pubsub::DynPublisher;
use embassy_time::{Duration, Timer};
use esp_hal::Blocking;
use esp_hal::analog::adc::{Adc, AdcChannel, AdcPin};
use esp_hal::peripherals::{ADC1, GPIO32, GPIO33, GPIO34, GPIO35};

use crate::config::SENSOR_INTERVAL_SECS;
use core::sync::atomic::Ordering;

use crate::drivers::dht22::{Dht22, Dht22Error};
use crate::drivers::tds::{average, linear_scale};
use crate::events::{DHT_ERROR, Measurements};
use crate::health::{Beat, beat};
use crate::report::{DHT_CHECKSUM, DHT_NO_RESPONSE, DHT_OK};

/// Lecturas del ADC que se promedian por canal.
const ADC_OVERSAMPLING: usize = 16;

#[task]
#[allow(clippy::too_many_arguments)]
pub async fn board_sensors_task(
    mut adc: Adc<'static, ADC1<'static>, Blocking>,
    mut light: AdcPin<GPIO34<'static>, ADC1<'static>>,
    mut ph: AdcPin<GPIO35<'static>, ADC1<'static>>,
    mut tds: AdcPin<GPIO32<'static>, ADC1<'static>>,
    mut soil: AdcPin<GPIO33<'static>, ADC1<'static>>,
    mut dht: Dht22<'static>,
    sensor_channel: DynPublisher<'static, Measurements>,
) {
    loop {
        beat(Beat::Analog);

        // Un fallo del DHT22 no publica nada: la lectura caduca tras tres
        // periodos y la tabla muestra «--» con el motivo, como en MicroPython.
        match dht.read() {
            Ok((temperature, humidity)) => {
                DHT_ERROR.store(DHT_OK, Ordering::Relaxed);
                sensor_channel.publish_immediate(Measurements::Air((temperature, humidity)));
            }
            Err(Dht22Error::Checksum) => DHT_ERROR.store(DHT_CHECKSUM, Ordering::Relaxed),
            Err(_) => DHT_ERROR.store(DHT_NO_RESPONSE, Ordering::Relaxed),
        }

        if let Some(raw) = oversample(&mut adc, &mut light) {
            let lux = linear_scale(raw, 0.0, 2000.0);
            sensor_channel.publish_immediate(Measurements::BH1750(lux));
        }
        if let Some(raw) = oversample(&mut adc, &mut ph) {
            sensor_channel.publish_immediate(Measurements::PH(linear_scale(raw, 0.0, 14.0)));
        }
        if let Some(raw) = oversample(&mut adc, &mut tds) {
            sensor_channel.publish_immediate(Measurements::TDS(linear_scale(raw, 0.0, 3000.0)));
        }
        if let Some(raw) = oversample(&mut adc, &mut soil) {
            let value = linear_scale(raw, 0.0, 100.0);
            sensor_channel.publish_immediate(Measurements::SoilMoisture(value));
        }

        Timer::after(Duration::from_secs(SENSOR_INTERVAL_SECS)).await;
    }
}

/// Lee varias veces un canal y devuelve el promedio, o `None` si ninguna
/// lectura fue válida.
fn oversample<PIN>(
    adc: &mut Adc<'static, ADC1<'static>, Blocking>,
    pin: &mut AdcPin<PIN, ADC1<'static>>,
) -> Option<u16>
where
    PIN: AdcChannel,
{
    let mut samples = [0u16; ADC_OVERSAMPLING];
    let mut count = 0;
    for slot in samples.iter_mut() {
        if let Ok(value) = nb::block!(adc.read_oneshot(pin)) {
            *slot = value;
            count += 1;
        }
    }
    average(&samples[..count])
}
