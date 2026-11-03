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

#[task]
pub async fn bme280_sequential_task(
    tca: Tca9548a<I2c<'static, Async>>,
    sensor_channel: DynPublisher<'static, Measurements>,
) {
    let [_, _, ch2, ch3, ch4, _, _, _] = tca.split();

    let address = 0x76;
    let mut bme1 = crate::drivers::bme280::Bme280::new(ch2, address)
        .await
        .unwrap();
    let mut bme2 = crate::drivers::bme280::Bme280::new(ch3, address)
        .await
        .unwrap();
    let mut bme3 = crate::drivers::bme280::Bme280::new(ch4, address)
        .await
        .unwrap();

    loop {
        if let Ok(m) = bme1.measure().await {
            debug!("BME280 ch2: temp={}, hum={}, press={}", m.temperature, m.humidity, m.pressure);
            sensor_channel.publish_immediate(Measurements::BME280((
                m.temperature,
                m.humidity,
                m.pressure,
            )));
        }

        if let Ok(m) = bme2.measure().await {
            debug!("BME280 ch3: temp={}, hum={}, press={}", m.temperature, m.humidity, m.pressure);
            sensor_channel.publish_immediate(Measurements::BME280((
                m.temperature,
                m.humidity,
                m.pressure,
            )));
        }

        if let Ok(m) = bme3.measure().await {
            debug!("BME280 ch4: temp={}, hum={}, press={}", m.temperature, m.humidity, m.pressure);
            sensor_channel.publish_immediate(Measurements::BME280((
                m.temperature,
                m.humidity,
                m.pressure,
            )));
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

#[task]
pub async fn bh1750_task(
    i2c_bus: I2cDevice<'static, NoopRawMutex, I2c<'static, Async>>,
    sensor_channel: DynPublisher<'static, Measurements>,
) {
    info!("Iniciando sensor de luminosidad BH1750...");
    let delay = embassy_time::Delay;
    let mut sensor = Bh1750Async::new(i2c_bus, delay, bh1750_embedded::Address::Low);

    loop {
        let lux: f32 = match sensor.one_time_measurement(Resolution::High).await {
            Ok(measure) => measure,
            Err(_) => -1.0,
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
