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

                    sensor_channel.publish_immediate(Measurements::BME280((
                        temperature,
                        humidity,
                        pressure,
                    )));

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

        if let Some(temperature) = reference_temperature {
            BME_TEMPERATURE.sender().send(temperature);
        }

        Timer::after(Duration::from_secs(10)).await;
    }
}

#[task]
pub async fn bme280_task(
    i2c_bus: I2cDevice<'static, NoopRawMutex, I2c<'static, Async>>,
    sensor_channel: DynPublisher<'static, Measurements>,
    address: u8,
) {
    wait_for_i2c_scan().await;

    info!("Starting BME280 sensor at address 0x{:02X}...", address);
    if let Ok(mut bme280) = Bme280::new(i2c_bus, address).await {
        let mut filters = BmeCascade::new();

        loop {
            match bme280.measure().await {
                Ok(measurement) => {
                    let (temperature, humidity, pressure) = filters.update(&measurement);
                    sensor_channel.publish_immediate(Measurements::BME280((
                        temperature,
                        humidity,
                        pressure,
                    )));
                    BME_TEMPERATURE.sender().send(temperature);
                }
                Err(_) => {
                    error!("Error reading BME280");
                }
            }
            Timer::after(Duration::from_secs(5)).await;
        }
    } else {
        error!("Could not initialize BME280 at 0x{:02X}", address);
    }
}

#[task]
pub async fn bme280_task_tca(
    i2c_bus: TcaI2c,
    sensor_channel: DynPublisher<'static, Measurements>,
    address: u8,
) {
    wait_for_i2c_scan().await;

    let channel = i2c_bus.get_channel();
    info!("Starting BME280 on TCA channel {}...", channel);
    if let Ok(mut bme280) = Bme280::new(i2c_bus, address).await {
        let mut filters = BmeCascade::new();

        loop {
            if let Ok(measurement) = bme280.measure().await {
                let (temperature, humidity, pressure) = filters.update(&measurement);
                sensor_channel.publish_immediate(Measurements::BME280((
                    temperature,
                    humidity,
                    pressure,
                )));
                BME_TEMPERATURE.sender().send(temperature);
            }
            Timer::after(Duration::from_secs(5)).await;
        }
    } else {
        error!("Could not initialize BME280 on TCA channel {}", channel);
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
    let mut lux_filter = SensorCascade::new(0.5, 2.0);
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

        Timer::after(Duration::from_secs(10)).await;
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

        if let Ok(val) = nb::block!(adc.read_oneshot(&mut tds)) {
            let tds_val = tds_filter.update(tds_from_adc(val, current_temp));
            sensor_channel.publish_immediate(Measurements::TDS(tds_val));
        }

        if let Ok(val) = nb::block!(adc.read_oneshot(&mut ph)) {
            let ph_val = ph_filter.update(ph_from_adc(val, current_temp));
            sensor_channel.publish_immediate(Measurements::PH(ph_val));
        }

        if let Ok(val) = nb::block!(adc.read_oneshot(&mut soil)) {
            let soil_val = soil_filter.update(soil_moisture_from_adc(val));
            sensor_channel.publish_immediate(Measurements::SoilMoisture(soil_val));
        }

        Timer::after(Duration::from_secs(10)).await;
    }
}
