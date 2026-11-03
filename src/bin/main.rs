#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use defmt::info;
use esp_hal::clock::CpuClock;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::timer::timg::TimerGroup;
use esp_println::{self as _, println};

#[cfg(any(feature = "sensors", feature = "actuators"))]
use esp_rtos::embassy::Executor;
#[cfg(any(feature = "sensors", feature = "actuators"))]
use esp_rtos::start_second_core;
#[cfg(any(feature = "wifi", feature = "sensors", feature = "actuators"))]
use static_cell::StaticCell;

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
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
    info!("Embassy initialized!");

    //
    // CORE 1: hardware local (sensores y/o actuadores)
    //
    #[cfg(any(feature = "sensors", feature = "actuators"))]
    {
        #[cfg(feature = "sensors")]
        let (i2c0, sda, scl, adc1, tds_pin, ph_pin) = (
            peripherals.I2C0,
            peripherals.GPIO21,
            peripherals.GPIO22,
            peripherals.ADC1,
            peripherals.GPIO34,
            peripherals.GPIO35,
        );

        #[cfg(feature = "actuators")]
        let (water_pin, lights_pin, humidifier_pin) =
            (peripherals.GPIO25, peripherals.GPIO26, peripherals.GPIO5);

        let cpu1_main = move |spawner: embassy_executor::Spawner| {
            #[cfg(feature = "sensors")]
            {
                use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
                use embassy_sync::blocking_mutex::raw::NoopRawMutex;
                use embassy_sync::mutex::Mutex;
                use esp_hal::Async;
                use esp_hal::analog::adc::{Adc, AdcConfig, Attenuation};
                use esp_hal::i2c::master::{Config, I2c};
                use smartpot_iot::drivers::tca9548a::Tca9548a;

                info!("Launching i2c sensor tasks");

                static I2C_BUS: StaticCell<Mutex<NoopRawMutex, I2c<'static, Async>>> =
                    StaticCell::new();
                let i2c = I2c::new(i2c0, Config::default())
                    .unwrap()
                    .with_scl(scl)
                    .with_sda(sda)
                    .into_async();
                let i2c_bus = I2C_BUS.init(Mutex::new(i2c));

                spawner.spawn(
                    smartpot_iot::i2c_scanner::scan_i2c(I2cDevice::new(i2c_bus))
                        .expect("Failed to scan I2C task"),
                );

                let tca = Tca9548a::new(i2c_bus, 0x70);
                spawner.spawn(
                    smartpot_iot::tasks::sensors::bme280_sequential_task(
                        tca,
                        smartpot_iot::events::SENSOR_CH.dyn_publisher().unwrap(),
                    )
                    .expect("Failed to launch bme280 sensors task"),
                );

                spawner.spawn(
                    smartpot_iot::tasks::sensors::bh1750_task(
                        I2cDevice::new(i2c_bus),
                        smartpot_iot::events::SENSOR_CH.dyn_publisher().unwrap(),
                    )
                    .expect("Failed to launch bh1750_sensors task"),
                );

                let mut adc1_config = AdcConfig::default();
                let tds = adc1_config.enable_pin(tds_pin, Attenuation::_11dB);
                let ph = adc1_config.enable_pin(ph_pin, Attenuation::_11dB);
                let adc_tds = Adc::new(adc1, adc1_config);

                spawner.spawn(
                    smartpot_iot::tasks::sensors::tds_task(
                        adc_tds,
                        tds,
                        ph,
                        smartpot_iot::events::SENSOR_CH.dyn_publisher().unwrap(),
                    )
                    .expect("Failed to launch tds task"),
                );
            }

            #[cfg(feature = "actuators")]
            {
                use esp_hal::gpio::{Level, Output, OutputConfig};

                let water = Output::new(water_pin, Level::Low, OutputConfig::default());
                let lights = Output::new(lights_pin, Level::Low, OutputConfig::default());
                let humidifier = Output::new(humidifier_pin, Level::Low, OutputConfig::default());

                spawner.spawn(
                    smartpot_iot::tasks::command::catch_commands(
                        smartpot_iot::events::COMMAND_CH.dyn_subscriber().unwrap(),
                        water,
                        lights,
                        humidifier,
                    )
                    .expect("Failed to create catch_commands task"),
                );
            }
        };

        #[allow(static_mut_refs)]
        {
            const CORE1_STACK_SIZE: usize = 8192;
            static CORE1_STACK: StaticCell<esp_hal::system::Stack<CORE1_STACK_SIZE>> =
                StaticCell::new();
            let core1_stack = CORE1_STACK.init_with(|| esp_hal::system::Stack::new());

            start_second_core(
                peripherals.CPU_CTRL,
                sw_int.software_interrupt1,
                core1_stack,
                || {
                    info!("Starting second cpu (CORE1)");
                    static EXECUTOR_CORE1: StaticCell<Executor> = StaticCell::new();
                    let exec = EXECUTOR_CORE1.init(Executor::new());

                    exec.run(|spawner: embassy_executor::Spawner| {
                        cpu1_main(spawner);
                    });
                },
            );
        }
    }

    //
    // CORE 0: red y transportes
    //
    #[cfg(feature = "wifi")]
    {
        use embassy_net::{DhcpConfig, StackResources};
        use esp_hal::rng::Trng;
        use esp_radio::wifi::ControllerConfig;
        use smartpot_iot::tasks::wifi::{net_task, wifi_connection_task};

        info!("Setting up network stack");

        let (wifi_controller, interfaces) =
            esp_radio::wifi::new(peripherals.WIFI, ControllerConfig::default())
                .expect("Could not initialize wifi controller");
        let wifi_interface = interfaces.station;

        static STACK_RESOURCES: StaticCell<StackResources<5>> = StaticCell::new();
        let stack_resources = STACK_RESOURCES.init_with(|| StackResources::<5>::new());

        let mut trng_buf = [0u8; 16];
        let trng = Trng::try_new().unwrap();
        trng.read(&mut trng_buf);

        let net_seed = trng.random() as u64 | ((trng.random() as u64) << 32);
        let config = embassy_net::Config::dhcpv4(DhcpConfig::default());
        let (stack, runner) = embassy_net::new(wifi_interface, config, stack_resources, net_seed);

        spawner.spawn(net_task(runner).expect("Error in net_task"));
        spawner.spawn(
            wifi_connection_task(wifi_controller, SSID, PASSWORD)
                .expect("Error in wifi_connection"),
        );

        #[cfg(feature = "mqtt")]
        {
            use smartpot_iot::events::COMMAND_CH;
            use smartpot_iot::events::telemetry::TELEMETRY_CH;
            use smartpot_iot::tasks::mqtt::mqtt_task;

            spawner.spawn(
                mqtt_task(
                    stack,
                    trng.clone(),
                    TELEMETRY_CH.dyn_subscriber().unwrap(),
                    COMMAND_CH.dyn_publisher().unwrap(),
                )
                .expect("Error Running MQTT task"),
            );
        }
    }

    //
    // TELEMETRIA: sensores + algun consumidor implementado.
    // Por ahora el unico consumidor real es MQTT.
    //
    #[cfg(all(feature = "sensors", feature = "mqtt"))]
    {
        use smartpot_iot::events::SENSOR_CH;
        use smartpot_iot::events::telemetry::TELEMETRY_CH;
        use smartpot_iot::tasks::telemetry::telemetry_task;

        spawner.spawn(
            telemetry_task(
                SENSOR_CH.dyn_subscriber().unwrap(),
                TELEMETRY_CH.dyn_publisher().unwrap(),
            )
            .expect("Error in telemetry"),
        );
    }

    loop {
        core::future::pending::<()>().await;
    }
}
