//! Tareas de adquisición de sensores (I2C: BME280, BH1750; Analógicos: TDS, pH, Humedad de suelo).

use bh1750_embedded::r#async::Bh1750Async;
use bh1750_embedded::Resolution;
use defmt::{debug, error, info};
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_executor::task;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::pubsub::DynPublisher;
use embassy_time::{Duration, Timer};
use esp_hal::analog::adc::{Adc, AdcPin};
use esp_hal::peripherals::{ADC1, GPIO32, GPIO34, GPIO35};
use esp_hal::Blocking;
use esp_hal::{i2c::master::I2c, Async};

use crate::drivers::tca9548a::{Tca9548a, TcaChannel};
use crate::drivers::tds::{ph_from_adc, soil_moisture_from_adc, tds_from_adc};
use crate::events::Measurements;

/// Try to initialize a BME280. A missing or misbehaving sensor is not fatal:
/// the caller simply skips that channel while the rest keep working.
async fn try_bme280<I2C>(i2c: I2C, address: u8, label: &str) -> Option<crate::drivers::bme280::Bme280<I2C>>
where
    I2C: embedded_hal_async::i2c::I2c,
{
    match crate::drivers::bme280::Bme280::new(i2c, address).await {
        Ok(sensor) => {
            info!("BME280 {} initialized on 0x{:02x}", label, address);
            Some(sensor)
        }
        Err(_e) => {
            error!("BME280 {} not available on 0x{:02x}; skipping channel", label, address);
            None
        }
    }
}

#[task]
pub async fn bme280_sequential_task(
    tca: Tca9548a<I2c<'static, Async>>,
    sensor_channel: DynPublisher<'static, Measurements>,
) {
    let [_, _, ch2, ch3, ch4, _, _, _] = tca.split();

    let address = 0x76;
    let mut bme1 = try_bme280(ch2, address, "channel 2").await;
    let mut bme2 = try_bme280(ch3, address, "channel 3").await;
    let mut bme3 = try_bme280(ch4, address, "channel 4").await;

    if bme1.is_none() && bme2.is_none() && bme3.is_none() {
        error!("No BME280 sensors detected; sensor task will stay idle");
    }

    loop {
        if let Some(sensor) = bme1.as_mut() {
            match sensor.measure().await {
                Ok(m) => {
                    debug!("Sending BME280:2 measurementes: {}", &m);
                    sensor_channel.publish_immediate(Measurements::BME280((m.temperature, m.humidity, m.pressure)));}
                Err(_e) => {
                    error!("Error measuring BME280:2 sensor on {}", address);
                }
            }
        }

        if let Some(sensor) = bme2.as_mut() {
            match sensor.measure().await {
                Ok(m) => {
                    debug!("Sending BME280:3 measurementes {}", &m);
                    sensor_channel.publish_immediate(Measurements::BME280((m.temperature, m.humidity, m.pressure)));}
                Err(_e) => {
                    error!("Error measuring BME280:3 sensor on {}", address);
                }
            }
        }

        if let Some(sensor) = bme3.as_mut() {
            match sensor.measure().await {
                Ok(m) => {
                    debug!("Sending BME280:4 measurementes {}", &m);
                    sensor_channel.publish_immediate(Measurements::BME280((m.temperature, m.humidity, m.pressure)));}
                Err(_e) => {
                    error!("Error measuring BME280:4 sensor on {}", address);
                }
            }
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
    info!("Iniciando sensor BME280 en dirección 0x{:02X}...", address);
    if let Ok(mut bme280) = crate::drivers::bme280::Bme280::new(i2c_bus, address).await {
        loop {
            match bme280.measure().await {
                Ok(m) => {
                    sensor_channel.publish_immediate(Measurements::BME280((
                        m.temperature,
                        m.humidity,
                        m.pressure,
                    )));
                }
                Err(_) => {
                    error!("Error al leer BME280");
                }
            }
            Timer::after(Duration::from_secs(5)).await;
        }
    } else {
        error!("No se pudo inicializar BME280 en 0x{:02X}", address);
    }
}

#[task]
pub async fn bme280_task_tca(
    i2c_bus: TcaChannel<I2cDevice<'static, NoopRawMutex, I2c<'static, Async>>>,
    sensor_channel: DynPublisher<'static, Measurements>,
    address: u8,
) {
    let channel = i2c_bus.get_channel();
    info!("Iniciando BME280 en TCA canal {}...", channel);
    if let Ok(mut bme280) = crate::drivers::bme280::Bme280::new(i2c_bus, address).await {
        loop {
            if let Ok(m) = bme280.measure().await {
                sensor_channel.publish_immediate(Measurements::BME280((
                    m.temperature,
                    m.humidity,
                    m.pressure,
                )));
            }
            Timer::after(Duration::from_secs(5)).await;
        }
    }
}

// BH1750 in one-time high-resolution mode. The conversion below is the same
// one used by `bh1750-embedded` (raw / 1.2 for the default MTreg). It is done
// here to avoid an LLVM 21 Xtensa backend bug when selecting a constant pool
// with two `f32` values.
const BH1750_ADDRESS: u8 = 0x23;
const BH1750_ONE_TIME_HIGH_RES: u8 = 0x20;
const BH1750_MEASUREMENT_MS: u64 = 180;

async fn bh1750_measure_lux<I>(i2c: &mut I) -> Result<f32, I::Error>
where
    I: embedded_hal_async::i2c::I2c,
{
    i2c.write(BH1750_ADDRESS, &[BH1750_ONE_TIME_HIGH_RES]).await?;
    Timer::after(Duration::from_millis(BH1750_MEASUREMENT_MS)).await;

    let mut buf = [0u8; 2];
    i2c.read(BH1750_ADDRESS, &mut buf).await?;

    Ok(u16::from_be_bytes(buf) as f32 / 1.2)
}

#[task]
pub async fn bh1750_task(
    i2c_bus: I2cDevice<'static, NoopRawMutex, I2c<'static, Async>>,
    sensor_channel: DynPublisher<'static, Measurements>,
) {
    info!("Iniciando sensor de luminosidad BH1750...");
    let delay = embassy_time::Delay;
    let mut sensor = Bh1750Async::new(i2c_bus, delay, bh1750_embedded::Address::Low);

    loop {
        debug!("Measuring BH1750 sensor");
        let lux: f32 = match bh1750_measure_lux(&mut i2c_bus).await {
            Ok(measure) => { measure }
            Err(_e) => { error!("Measuring BH1750 sensor:"); -1.0}
        };

        if lux >= 0.0 {
            sensor_channel.publish_immediate(Measurements::BH1750(lux));
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
    info!("Iniciando sensores analógicos (TDS, pH, Humedad de suelo)...");
    let temperature: f32 = 25.0;

    loop {
        if let Ok(val) = nb::block!(adc.read_oneshot(&mut tds)) {
            let tds_val = tds_from_adc(val, temperature);
            sensor_channel.publish_immediate(Measurements::TDS(tds_val));
        }

        if let Ok(val) = nb::block!(adc.read_oneshot(&mut ph)) {
            let ph_val = ph_from_adc(val, temperature);
            sensor_channel.publish_immediate(Measurements::PH(ph_val));
        }

        if let Ok(val) = nb::block!(adc.read_oneshot(&mut soil)) {
            let soil_val = soil_moisture_from_adc(val);
            sensor_channel.publish_immediate(Measurements::SoilMoisture(soil_val));
        }

        Timer::after(Duration::from_secs(10)).await;
    }
}