#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use defmt::{debug, info};
use esp_hal::analog::adc::{Adc, AdcConfig, Attenuation};
use esp_hal::clock::CpuClock;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::rng::Trng;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::{
    i2c::master::{Config, I2c},
    Async,
};
use esp_println::{self as _, println};
use esp_radio::wifi::ControllerConfig;
use esp_rtos::embassy::Executor;
use esp_rtos::start_second_core;
use static_cell::StaticCell;

use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_net::{DhcpConfig, StackResources};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use smartpot_iot::config::DEFAULT_CONFIG;
use smartpot_iot::drivers::tca9548a::Tca9548a;

#[cfg(feature = "sensors")]
use smartpot_iot::events::SENSOR_CH;

#[cfg(feature = "actuators")]
use smartpot_iot::events::{ACK_CH, COMMAND_CH};

extern crate alloc;

esp_bootloader_esp_idf::esp_app_desc!();

#[cfg(feature = "wifi")]
pub const SSID: &str = env!("SSID");
#[cfg(feature = "wifi")]
pub const PASSWORD: &str = env!("PASSWORD");

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("\n\n================ PANIC ================");
    if let Some(location) = info.location() {
        println!(
            "Panic in file '{}' at line {}",
            location.file(),
            location.line(),
        );
    } else {
        println!("Panic but no location information available.");
    }

    println!("message: {:?}", info.message());
    println!("======================================\n\n");

    loop {}
}

#[allow(unused_variables)]
#[esp_rtos::main]
async fn main(spawner: embassy_executor::Spawner) -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    esp_alloc::heap_allocator!(size: 98768);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);
    info!("Embassy RTOS initialized!");

    // =========================================================================
    // CORE 1: ADQUISICIÓN DE SENSORES Y CONTROL DE ACTUADORES
    // =========================================================================

    // Pines de protocolo I2C
    let i2c0 = peripherals.I2C0;
    let sda = peripherals.GPIO21;
    let scl = peripherals.GPIO22;

    // Sensores analógicos (ADC1)
    let mut adc1_config = AdcConfig::default();
    let tds_pin = peripherals.GPIO34;
    let tds = adc1_config.enable_pin(tds_pin, Attenuation::_11dB);

    let ph_pin = peripherals.GPIO35;
    let ph = adc1_config.enable_pin(ph_pin, Attenuation::_11dB);

    let soil_pin = peripherals.GPIO32;
    let soil = adc1_config.enable_pin(soil_pin, Attenuation::_11dB);

    let adc1 = Adc::new(peripherals.ADC1, adc1_config);

    // Pines de actuadores (Relés / Transistores)
    let water_pin = peripherals.GPIO25;
    let lights_pin = peripherals.GPIO26;
    let fan_pin = peripherals.GPIO27;
    let humidifier_pin = peripherals.GPIO5;

    let cpu1_main = move |_spawner: embassy_executor::Spawner| {
        debug!("Iniciando tareas de sensores y actuadores en CORE 1");
        static I2C_BUS: StaticCell<Mutex<NoopRawMutex, I2c<'static, Async>>> = StaticCell::new();
        let i2c = I2c::new(i2c0, Config::default())
            .unwrap()
            .with_scl(scl)
            .with_sda(sda)
            .into_async();
        let _i2c_bus = I2C_BUS.init(Mutex::new(i2c));

        #[cfg(feature = "sensors")]
        {
            _spawner
                .spawn(
                    smartpot_iot::i2c_scanner::scan_i2c(I2cDevice::new(_i2c_bus))
                        .expect("Fallo al crear scan_i2c"),
                );

            let tca = Tca9548a::new(_i2c_bus, 0x70);
            _spawner
                .spawn(
                    smartpot_iot::tasks::sensors::bme280_sequential_task(
                        tca,
                        SENSOR_CH.dyn_publisher().unwrap(),
                    )
                    .expect("Fallo al crear bme280_sequential_task"),
                );

            _spawner
                .spawn(
                    smartpot_iot::tasks::sensors::bh1750_task(
                        I2cDevice::new(_i2c_bus),
                        SENSOR_CH.dyn_publisher().unwrap(),
                    )
                    .expect("Fallo al crear bh1750_task"),
                );

            _spawner
                .spawn(
                    smartpot_iot::tasks::sensors::analog_sensors_task(
                        adc1,
                        tds,
                        ph,
                        soil,
                        SENSOR_CH.dyn_publisher().unwrap(),
                    )
                    .expect("Fallo al crear analog_sensors_task"),
                );
        }

        #[cfg(feature = "actuators")]
        {
            use esp_hal::gpio::{Level, Output, OutputConfig};

            let water = Output::new(water_pin, Level::Low, OutputConfig::default());
            let lights = Output::new(lights_pin, Level::Low, OutputConfig::default());
            let fan = Output::new(fan_pin, Level::Low, OutputConfig::default());
            let humidifier = Output::new(humidifier_pin, Level::Low, OutputConfig::default());

            _spawner
                .spawn(
                    smartpot_iot::tasks::command::catch_commands(
                        COMMAND_CH.dyn_subscriber().unwrap(),
                        ACK_CH.dyn_publisher().unwrap(),
                        water,
                        lights,
                        fan,
                        humidifier,
                    )
                    .expect("Fallo al crear catch_commands"),
                );
        }
    };

    #[allow(static_mut_refs)]
    {
        const CORE1_STACK_SIZE: usize = 8192;
        static CORE1_STACK: StaticCell<esp_hal::system::Stack<CORE1_STACK_SIZE>> = StaticCell::new();
        let core1_stack = CORE1_STACK.init_with(esp_hal::system::Stack::new);

        start_second_core(
            peripherals.CPU_CTRL,
            sw_int.software_interrupt1,
            core1_stack,
            || {
                info!("Iniciando segundo núcleo (CORE1)");
                static EXECUTOR_CORE1: StaticCell<Executor> = StaticCell::new();
                let exec = EXECUTOR_CORE1.init(Executor::new());

                exec.run(|spawner: embassy_executor::Spawner| {
                    cpu1_main(spawner);
                });
            },
        );
    }

    // =========================================================================
    // CORE 0: PILA DE RED WI-FI Y CLIENTE MQTT (CONTRATO V1)
    // =========================================================================

    #[cfg(feature = "wifi")]
    {
        use embassy_net::{DhcpConfig, StackResources};
        use esp_hal::rng::Trng;
        use esp_radio::wifi::ControllerConfig;
        use smartpot_iot::tasks::wifi::{net_task, wifi_connection_task};

        #[cfg(any(feature = "sensors", feature = "actuators", feature = "mqtt", feature = "http-api", feature = "ble"))]
        use smartpot_iot::{
            events::telemetry::TELEMETRY_CH,
            tasks::telemetry::telemetry_task
        };

        info!("Configurando pila de red Wi-Fi y radio...");
        let (wifi_controller, interfaces) =
            esp_radio::wifi::new(peripherals.WIFI, ControllerConfig::default())
                .expect("Could not initialize wifi controller");
        let wifi_interface = interfaces.station;

        static STACK_RESOURCES: StaticCell<StackResources<5>> = StaticCell::new();
        let stack_resources = STACK_RESOURCES.init_with(StackResources::<5>::new);

        let mut trng_buf = [0u8; 16];
        // let _trng_source = TrngSource::new(peripherals.RNG, peripherals.ADC1.reborrow());
        let trng = Trng::try_new().unwrap();
        trng.read(&mut trng_buf);

        let net_seed = trng.random() as u64 | ((trng.random() as u64) << 32);
        let net_config = embassy_net::Config::dhcpv4(DhcpConfig::default());
        let (_stack, runner) =
            embassy_net::new(wifi_interface, net_config, stack_resources, net_seed);

        spawner.spawn(net_task(runner).expect("Fallo al iniciar net_task"));
        spawner.spawn(
            wifi_connection_task(
                wifi_controller,
                DEFAULT_CONFIG.wifi_ssid,
                DEFAULT_CONFIG.wifi_password,
            )
            .expect("Fallo al iniciar wifi_connection_task"),
        );

        spawner.spawn(
            telemetry_task(
                SENSOR_CH.dyn_subscriber().unwrap(),
                TELEMETRY_CH.dyn_publisher().unwrap(),
                DEFAULT_CONFIG.telemetry_interval_secs,
            )
            .expect("Fallo al iniciar telemetry_task"),
        );

        #[cfg(all(feature = "mqtt", feature = "actuators", feature = "sensors"))]
        spawner.spawn(
            smartpot_iot::tasks::mqtt::mqtt_task(
                _stack,
                trng.clone(),
                DEFAULT_CONFIG,
                TELEMETRY_CH.dyn_subscriber().unwrap(),
                COMMAND_CH.dyn_publisher().unwrap(),
                ACK_CH.dyn_subscriber().unwrap(),
            )
            .expect("Fallo al iniciar mqtt_task"),
        );
    }

    loop {
        /*
        info!("Free heap: {} bytes", esp_alloc::HEAP.free());
        info!("Used: {} bytes", esp_alloc::HEAP.used());
        info!("stats: {} ", esp_alloc::HEAP.stats());
        embassy_time::Timer::after(embassy_time::Duration::from_secs(5)).await;
        */
        core::future::pending::<()>().await;
    }
}
