use esp_hal::analog::adc::AdcPin;
use esp_hal::peripherals::ADC1;
use defmt::{info, error, debug};
use esp_hal::{i2c::master::I2c, Async};
use esp_hal::Blocking;
use embassy_executor::task;
use embassy_time::{Timer, Duration};
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex};
use embassy_sync::pubsub::DynPublisher;

use crate::events::Measurements;
use esp_hal::analog::adc::Adc;
use esp_hal::peripherals::{GPIO34, GPIO35};
use crate::drivers::tca9548a::{Tca9548a, TcaChannel};

// TODO: dynamic recon of devices
// TODO: Configurable measure time

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
pub async fn bme280_sequential_task(tca: Tca9548a<I2c<'static, Async>> , sensor_channel: DynPublisher<'static, Measurements>) {
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
pub async fn bme280_task(i2c_bus: I2cDevice<'static, NoopRawMutex, I2c<'static, Async>>, sensor_channel: DynPublisher<'static, Measurements>, address: u8 ) {
    bme280(i2c_bus, sensor_channel, address, None).await;
}

#[task]
pub async fn bme280_task_tca(i2c_bus: TcaChannel<I2cDevice<'static, NoopRawMutex, I2c<'static, Async>>>, sensor_channel: DynPublisher<'static, Measurements>, address: u8 ) {
    let channel = Some(i2c_bus.get_channel());
    bme280(i2c_bus, sensor_channel, address, channel).await;
}

async fn bme280<I2C>(
    i2c_bus: I2C,
    sensor_channel: DynPublisher<'static, Measurements>,
    address: u8,
    channel: Option<u8>
)
where I2C: embedded_hal_async::i2c::I2c
{
    info!("Starting BME280 sensor task...");
    let mut bme280 = crate::drivers::bme280::Bme280::new(i2c_bus, address).await.unwrap();

    loop {
        if let Some(c) = channel {
            info!("Measuring BME280 sensor from channel {}", c);
        } else {
            info!("Measuring BME280 sensor on default");
        }

        match bme280.measure().await {
            Ok(m) => {
                info!("Sending BME280:{} measurementes {}", channel, &m);
                sensor_channel.publish_immediate(Measurements::BME280((m.temperature, m.humidity, m.pressure)));}
            Err(_) => {
                error!("Error measuring BME280:{} sensor on", channel);
            }
        }

        Timer::after(Duration::from_secs(2)).await;
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
pub async fn bh1750_task(mut i2c_bus: I2cDevice<'static, NoopRawMutex, I2c<'static, Async>>, sensor_channel: DynPublisher<'static, Measurements>) {
    info!("Starting BM1750 sensor task...");

    loop {
        debug!("Measuring BH1750 sensor");
        let lux: f32 = match bh1750_measure_lux(&mut i2c_bus).await {
            Ok(measure) => { measure }
            Err(_e) => { error!("Measuring BH1750 sensor:"); -1.0}
        };

        debug!("Sending BH1750 lux measure {}", lux);
        sensor_channel.publish_immediate(Measurements::BH1750(lux));

        Timer::after(Duration::from_secs(10)).await;
    }
}

pub fn tds_from_adc(
    adc_value: u16,
    temperature: f32,
) -> f32 {
    const VREF: f32 = 3.3;

    // ADC ESP32 12 bits
    let voltage =
        adc_value as f32 * VREF / 4095.0;

    // Compensación temperatura
    let compensation_coefficient =
        1.0 + 0.02 * (temperature - 25.0);

    let compensation_voltage =
        voltage / compensation_coefficient;

    // Fórmula TDS
    (
        133.42 * compensation_voltage*compensation_voltage*compensation_voltage
            - 255.86 * compensation_voltage*compensation_voltage
            + 857.39 * compensation_voltage
    ) * 0.5
}

pub fn ph_from_adc(
    adc_value: u16,
    temperature: f32,
) -> f32 {
    const VREF: f32 = 3.3;

    // ADC ESP32 12 bits
    let voltage =
        adc_value as f32 * VREF / 4095.0;

    // Compensación temperatura
    let compensation_coefficient =
        1.0 + 0.02 * (temperature - 25.0);

    let compensated_voltage =
        voltage / compensation_coefficient;

    // Conversión pH
    7.0 + ((2.5 - compensated_voltage) / 0.18)
}

#[task]
pub async fn tds_task(
    mut adc: Adc<'static, ADC1<'static>, Blocking>,
    mut tds: AdcPin<GPIO34<'static>, ADC1<'static>>,
    mut ph: AdcPin<GPIO35<'static>, ADC1<'static>>,
    sensor_channel: DynPublisher<'static, Measurements>,
) {
    let temperature: f32 = 25.0;

    loop {
        let value: u16 = nb::block!(adc.read_oneshot(&mut tds)).unwrap();
        let tds_value = tds_from_adc(value, temperature);

        sensor_channel.publish_immediate(Measurements::TDS(tds_value));

        let value: u16 = nb::block!(adc.read_oneshot(&mut ph)).unwrap();
        let ph_value = ph_from_adc(value, temperature);

        sensor_channel.publish_immediate(Measurements::PH(ph_value));

        Timer::after(Duration::from_secs(10)).await;
    }
}