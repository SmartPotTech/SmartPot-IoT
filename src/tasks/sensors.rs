//! Tareas de adquisición de sensores (I2C: BME280, BH1750; Analógicos: TDS, pH, Humedad de suelo).
//!
//! Todas las lecturas pasan por una `SensorCascade` (mediana + Kalman Q16.16)
//! antes de publicarse en `SENSOR_CH`. Las tareas I2C esperan a que el escáner
//! termine; `scan_i2c` se lanza siempre junto a estas tareas con la feature
//! `sensors`. Si el escáner no llegara a publicar, un tiempo límite de 5 s
//! desbloquea las tareas para no detener la adquisición.

use crate::drivers::bh1750::{BH1750_DEFAULT_ADDRESS, Bh1750};
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_executor::task;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::pubsub::DynPublisher;
use embassy_time::{Duration, Timer, with_timeout};
use esp_hal::Blocking;
use esp_hal::analog::adc::{Adc, AdcPin};
use esp_hal::peripherals::{ADC1, GPIO32, GPIO34, GPIO35};
use esp_hal::{Async, i2c::master::I2c};

use crate::config::SENSOR_INTERVAL_SECS;
use crate::drivers::bme280::{Bme280, MeasurementsBME280};
use crate::drivers::tca9548a::{Tca9548a, TcaChannel};
use crate::drivers::tds::{ph_from_adc, soil_moisture_from_adc, tds_from_adc};
use crate::events::{BME_TEMPERATURE, I2C_SCAN_DONE, Measurements};
use crate::filter::SensorCascade;
use crate::health::{Beat, beat};

/// Dirección I2C de los tres BME280 (configurable por hardware, todos iguales).
const BME280_ADDRESS: u8 = 0x76;

/// Canales del TCA9548A usados por los tres BME280.
const BME280_TCA_CHANNELS: [u8; 3] = [2, 3, 4];

/// Filtros de una unidad BME280: temperatura, humedad y presión.
#[derive(Clone, Copy)]
struct BmeCascade {
    temperature: SensorCascade,
    humidity: SensorCascade,
    pressure: SensorCascade,
}

impl BmeCascade {
    const fn new() -> Self {
        Self {
            temperature: SensorCascade::new(0.01, 0.1),
            humidity: SensorCascade::new(0.05, 0.5),
            pressure: SensorCascade::new(0.02, 0.4),
        }
    }

    fn update(&mut self, measurement: &MeasurementsBME280) -> (f32, f32, f32) {
        (
            self.temperature.update(measurement.temperature),
            self.humidity.update(measurement.humidity),
            self.pressure.update(measurement.pressure),
        )
    }
}

type TcaI2c = TcaChannel<I2c<'static, Async>>;

/// Espera a que el escáner I2C publique `true`. El valor se conserva aunque la
/// tarea arranque después del escaneo, así que no se pierde el evento.
async fn wait_for_i2c_scan() {
    let Some(mut receiver) = I2C_SCAN_DONE.receiver() else {
        // Sin receptor disponible no se bloquea la tarea: el escáner ya se
        // habrá lanzado antes y la operación I2C reintentará si hace falta.
        return;
    };

    if !receiver.try_get().unwrap_or(false) {
        // El escáner se lanza siempre con `sensors`, pero el tiempo límite
        // evita que un fallo de arranque deje estas tareas bloqueadas.
        let _ = with_timeout(Duration::from_secs(5), receiver.changed()).await;
    }
}

/// Inicializa los BME280 disponibles, sin abortar si alguno falta.
async fn init_bme_sensors(tca: Tca9548a<I2c<'static, Async>>) -> [Option<Bme280<TcaI2c>>; 3] {
    let [_, _, ch2, ch3, ch4, _, _, _] = tca.split();
    let channels = [ch2, ch3, ch4];
    let mut sensors: [Option<Bme280<TcaI2c>>; 3] = [None, None, None];

    for (index, channel) in channels.into_iter().enumerate() {
        match Bme280::new(channel, BME280_ADDRESS).await {
            Ok(sensor) => {
                sensors[index] = Some(sensor);
                info!(
                    "BME280 initialized on TCA channel {}",
                    BME280_TCA_CHANNELS[index]
                );
            }
            Err(_) => {
                warn!(
                    "BME280 initialization failed on TCA channel {}",
                    BME280_TCA_CHANNELS[index]
                );
            }
        }
    }

    sensors
}

#[task]
pub async fn bme280_sequential_task(
    tca: Tca9548a<I2c<'static, Async>>,
    sensor_channel: DynPublisher<'static, Measurements>,
) {
    wait_for_i2c_scan().await;

    let mut sensors = init_bme_sensors(tca).await;
    let available = sensors.iter().flatten().count();
    if available == 0 {
        warn!("No BME280 sensors available; analog compensation uses 25.0 C");
    } else {
        info!("BME280 sensors available: {}/3", available);
    }

    let mut filters = [BmeCascade::new(); 3];

    loop {
        beat(Beat::Bme280);
        // Se prefiere el primer sensor disponible (canal 2) como referencia
        // térmica para TDS y pH; si falla, se usa el siguiente que responda.
        let mut reference_temperature: Option<f32> = None;
        // Las tres unidades se promedian: antes se publicaba cada una por
        // separado y la telemetría se quedaba con la última que respondía.
        let mut sum = (0.0f32, 0.0f32, 0.0f32);
        let mut readings = 0u8;

        for index in 0..sensors.len() {
            let Some(sensor) = sensors[index].as_mut() else {
                continue;
            };

            match sensor.measure().await {
                Ok(measurement) => {
                    let (temperature, humidity, pressure) = filters[index].update(&measurement);

                    debug!(
                        "BME280 ch{}: temp={}, hum={}, press={}",
                        BME280_TCA_CHANNELS[index], temperature, humidity, pressure
                    );

                    sum = (sum.0 + temperature, sum.1 + humidity, sum.2 + pressure);
                    readings += 1;

                    if reference_temperature.is_none() {
                        reference_temperature = Some(temperature);
                    }
                }
                Err(_) => {
                    warn!(
                        "BME280 read failed on TCA channel {}",
                        BME280_TCA_CHANNELS[index]
                    );
                }
            }
        }

        if readings > 0 {
            let count = readings as f32;
            sensor_channel.publish_immediate(Measurements::BME280((
                sum.0 / count,
                sum.1 / count,
                sum.2 / count,
            )));
        }

        if let Some(temperature) = reference_temperature {
            BME_TEMPERATURE.sender().send(temperature);
        }

        Timer::after(Duration::from_secs(SENSOR_INTERVAL_SECS)).await;
    }
}

#[task]
pub async fn bh1750_task(
    i2c_bus: I2cDevice<'static, NoopRawMutex, I2c<'static, Async>>,
    sensor_channel: DynPublisher<'static, Measurements>,
) {
    wait_for_i2c_scan().await;

    info!("Starting BH1750 light sensor...");
    let mut sensor = Bh1750::new(i2c_bus, BH1750_DEFAULT_ADDRESS);
    // El BH1750 llega a 65 535 lx: se filtra en decenas de lux para no saturar Q16.16.
    let mut lux_filter = SensorCascade::with_scale(0.5, 2.0, 10.0);
    let mut available = true;

    loop {
        beat(Beat::Light);
        match sensor.measure().await {
            Ok(measure) => {
                if !available {
                    info!("BH1750 light sensor online");
                    available = true;
                }
                let lux = lux_filter.update(measure);
                sensor_channel.publish_immediate(Measurements::BH1750(lux));
            }
            Err(_) => {
                if available {
                    warn!(
                        "BH1750 light sensor not responding at 0x{:02X}",
                        BH1750_DEFAULT_ADDRESS
                    );
                    available = false;
                }
            }
        }

        Timer::after(Duration::from_secs(SENSOR_INTERVAL_SECS)).await;
    }
}

#[task]
pub async fn analog_sensors_task(
    mut adc: Adc<'static, ADC1<'static>, Blocking>,
    mut tds: AdcPin<GPIO34<'static>, ADC1<'static>>,
    mut ph: AdcPin<GPIO35<'static>, ADC1<'static>>,
    mut soil: AdcPin<GPIO32<'static>, ADC1<'static>>,
    sensor_channel: DynPublisher<'static, Measurements>,
) {
    info!("Starting analog sensors (TDS, pH, Soil Moisture)...");
    let mut temperature_receiver = BME_TEMPERATURE.receiver();

    let mut tds_filter = SensorCascade::new(0.5, 4.0);
    let mut ph_filter = SensorCascade::new(0.001, 0.01);
    let mut soil_filter = SensorCascade::new(0.1, 0.5);

    loop {
        beat(Beat::Analog);
        // Sin lectura de BME280 se usa el respaldo de 25.0 °C.
        let current_temp = temperature_receiver
            .as_mut()
            .and_then(|receiver| receiver.try_get())
            .unwrap_or(25.0);

        // Se promedian 16 lecturas por canal para reducir el ruido del ADC.
        let tds_raw = oversample(&mut adc, &mut tds);
        let ph_raw = oversample(&mut adc, &mut ph);
        let soil_raw = oversample(&mut adc, &mut soil);

        let tds_val = tds_raw
            .and_then(|raw| tds_from_adc(raw, current_temp))
            .map(|value| tds_filter.update(value));
        let ph_val = ph_raw
            .and_then(|raw| ph_from_adc(raw, current_temp))
            .map(|value| ph_filter.update(value));
        let soil_val = soil_raw.map(|raw| soil_filter.update(soil_moisture_from_adc(raw)));

        if let Some(value) = tds_val {
            sensor_channel.publish_immediate(Measurements::TDS(value));
        }
        if let Some(value) = ph_val {
            sensor_channel.publish_immediate(Measurements::PH(value));
        }
        if let Some(value) = soil_val {
            sensor_channel.publish_immediate(Measurements::SoilMoisture(value));
        }

        info!(
            "[ADC] TDS: {:?} ppm, pH: {:?}, Soil moisture: {:?}% (raw {:?}/{:?}/{:?})",
            tds_val, ph_val, soil_val, tds_raw, ph_raw, soil_raw
        );

        Timer::after(Duration::from_secs(SENSOR_INTERVAL_SECS)).await;
    }
}

/// Lecturas por canal que se promedian en cada ciclo.
const ADC_OVERSAMPLING: usize = 16;

/// Lee `ADC_OVERSAMPLING` veces un canal y devuelve el promedio, o `None` si
/// ninguna lectura fue válida.
fn oversample<PIN>(
    adc: &mut Adc<'static, ADC1<'static>, Blocking>,
    pin: &mut AdcPin<PIN, ADC1<'static>>,
) -> Option<u16>
where
    PIN: esp_hal::analog::adc::AdcChannel,
{
    let mut samples = [0u16; ADC_OVERSAMPLING];
    let mut count = 0;
    for slot in samples.iter_mut() {
        if let Ok(value) = nb::block!(adc.read_oneshot(pin)) {
            *slot = value;
            count += 1;
        }
    }
    crate::drivers::tds::average(&samples[..count])
}
