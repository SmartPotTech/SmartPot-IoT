//! Sensores de la placa compartida con el firmware MicroPython.
//!
//! DHT22 (temperatura y humedad) en GPIO15 y cuatro potenciómetros: luz
//! (0..2000 lx) en GPIO34, pH (0..14) en GPIO35, TDS (0..3000 ppm) en GPIO32 y
//! humedad de suelo (0..100 %) en GPIO33, con las mismas escalas lineales que
//! MicroPython. El filtrado mediana + Kalman es interno: con el cursor quieto el
//! valor publicado coincide con el de MicroPython.

use embassy_executor::task;
use embassy_sync::pubsub::DynPublisher;
use embassy_time::{Duration, Timer};
use esp_hal::Blocking;
use esp_hal::analog::adc::{Adc, AdcChannel, AdcPin};
use esp_hal::peripherals::{ADC1, GPIO32, GPIO33, GPIO34, GPIO35};

use crate::config::SENSOR_INTERVAL_SECS;
use crate::drivers::dht22::Dht22;
use crate::drivers::tds::{average, linear_scale};
use crate::events::Measurements;
use crate::filter::SensorCascade;
use crate::health::{Beat, beat};

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
    let mut temperature_filter = SensorCascade::new(0.01, 0.1);
    let mut humidity_filter = SensorCascade::new(0.05, 0.5);
    let mut light_filter = SensorCascade::new(0.5, 2.0);
    let mut ph_filter = SensorCascade::new(0.001, 0.01);
    let mut tds_filter = SensorCascade::new(0.5, 4.0);
    let mut soil_filter = SensorCascade::new(0.1, 0.5);

    loop {
        beat(Beat::Analog);

        // Un fallo del DHT22 no publica nada: la lectura caduca y la
        // instantánea muestra «--», como en MicroPython.
        if let Ok((temperature, humidity)) = dht.read() {
            sensor_channel.publish_immediate(Measurements::Air((
                temperature_filter.update(temperature),
                humidity_filter.update(humidity),
            )));
        }

        if let Some(raw) = oversample(&mut adc, &mut light) {
            let lux = light_filter.update(linear_scale(raw, 0.0, 2000.0));
            sensor_channel.publish_immediate(Measurements::BH1750(lux));
        }
        if let Some(raw) = oversample(&mut adc, &mut ph) {
            let value = ph_filter.update(linear_scale(raw, 0.0, 14.0));
            sensor_channel.publish_immediate(Measurements::PH(value));
        }
        if let Some(raw) = oversample(&mut adc, &mut tds) {
            let value = tds_filter.update(linear_scale(raw, 0.0, 3000.0));
            sensor_channel.publish_immediate(Measurements::TDS(value));
        }
        if let Some(raw) = oversample(&mut adc, &mut soil) {
            let value = soil_filter.update(linear_scale(raw, 0.0, 100.0));
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
