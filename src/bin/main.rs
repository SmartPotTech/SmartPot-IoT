#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![allow(clippy::large_stack_frames)]

use esp_backtrace as _;
#[cfg(feature = "board-physical")]
use esp_hal::Async;
use esp_hal::analog::adc::{Adc, AdcConfig, Attenuation};
use esp_hal::clock::CpuClock;
#[cfg(any(feature = "board-physical", feature = "display"))]
use esp_hal::i2c::master::{Config, I2c};
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::timer::timg::TimerGroup;
use esp_println::{self as _, println};
use esp_rtos::embassy::Executor;
use esp_rtos::start_second_core;
use smartpot_iot::{debug, info};
use static_cell::StaticCell;

#[cfg(feature = "board-physical")]
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
#[cfg(feature = "board-physical")]
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
#[cfg(feature = "wifi")]
use smartpot_iot::config::DEFAULT_CONFIG;
#[cfg(feature = "board-physical")]
use smartpot_iot::drivers::tca9548a::Tca9548a;

#[cfg(feature = "sensors")]
use smartpot_iot::events::SENSOR_CH;

#[cfg(feature = "actuators")]
use smartpot_iot::events::{ACK_CH, COMMAND_CH};

extern crate alloc;

esp_bootloader_esp_idf::esp_app_desc!();

/// Gancho de `esp-backtrace` antes de imprimir el pánico: lo primero es dejar
/// todos los actuadores apagados, porque el chip va a reiniciarse.
#[unsafe(no_mangle)]
pub fn custom_pre_backtrace() {
    smartpot_iot::safety::force_actuators_off();
    println!("\nSMARTPOT-FATAL panic");
}

/// Gancho final de `esp-backtrace`: en vez de quedarse en un bucle infinito,
/// reinicia el chip para que el dispositivo vuelva a operar solo.
#[unsafe(no_mangle)]
pub fn custom_halt() -> ! {
    esp_hal::delay::Delay::new().delay_millis(100);
    esp_hal::system::software_reset()
}

/// Código de `machine.reset_cause()` de MicroPython: 1 encendido, 2 externo,
/// 3 watchdog, 4 deep sleep y 5 reinicio por software o pánico.
fn reset_cause() -> u8 {
    use esp_hal::rtc_cntl::SocResetReason::*;
    match esp_hal::system::reset_reason() {
        Some(ChipPowerOn | SysBrownOut) | None => 1,
        Some(CoreMwdt0 | CoreMwdt1 | CoreRtcWdt | CpuMwdt0 | Cpu0RtcWdt | SysRtcWdt) => 3,
        Some(CoreDeepSleep) => 4,
        Some(CoreSw | Cpu0Sw | Cpu1Cpu0) => 5,
        Some(_) => 2,
    }
}

#[allow(unused_variables)]
#[esp_rtos::main]
async fn main(spawner: embassy_executor::Spawner) -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    esp_alloc::heap_allocator!(#[unsafe(link_section = ".dram2_uninit")] size: 98768);

    // Las salidas de actuador quedan en bajo desde el primer instante.
    smartpot_iot::safety::force_actuators_off();
    let rtc = esp_hal::rtc_cntl::Rtc::new(peripherals.LPWR);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);
    smartpot_iot::sim::enable_timer_interrupt_for_simulator();
    info!("Embassy RTOS initialized!");

    // =========================================================================
    // CORE 1: ADQUISICIÓN DE SENSORES Y CONTROL DE ACTUADORES
    // =========================================================================

    // Sensores analógicos (ADC1). Placa compartida con MicroPython: luz en
    // GPIO34, pH en GPIO35, TDS en GPIO32 y humedad de suelo en GPIO33.
    let mut adc1_config = AdcConfig::default();
    #[cfg(not(feature = "board-physical"))]
    let (light, ph, tds, soil) = (
        adc1_config.enable_pin(peripherals.GPIO34, Attenuation::_11dB),
        adc1_config.enable_pin(peripherals.GPIO35, Attenuation::_11dB),
        adc1_config.enable_pin(peripherals.GPIO32, Attenuation::_11dB),
        adc1_config.enable_pin(peripherals.GPIO33, Attenuation::_11dB),
    );
    // Prototipo físico: TDS en GPIO34, pH en GPIO35 y suelo en GPIO32.
    #[cfg(feature = "board-physical")]
    let (tds, ph, soil) = (
        adc1_config.enable_pin(peripherals.GPIO34, Attenuation::_11dB),
        adc1_config.enable_pin(peripherals.GPIO35, Attenuation::_11dB),
        adc1_config.enable_pin(peripherals.GPIO32, Attenuation::_11dB),
    );
    let adc1 = Adc::new(peripherals.ADC1, adc1_config);

    // Pines de actuadores (relés o transistores), según la placa.
    #[cfg(not(feature = "board-physical"))]
    let (water_pin, uv_light_pin, fan_pin) =
        (peripherals.GPIO19, peripherals.GPIO18, peripherals.GPIO5);
    #[cfg(feature = "board-physical")]
    let (water_pin, uv_light_pin, fan_pin, humidifier_pin) = (
        peripherals.GPIO25,
        peripherals.GPIO26,
        peripherals.GPIO27,
        peripherals.GPIO5,
    );

    let cpu1_main = move |_spawner: embassy_executor::Spawner| {
        debug!("Starting sensor and actuator tasks on CORE 1");

        // Placa compartida con MicroPython: DHT22 en GPIO15 y potenciómetros.
        #[cfg(all(feature = "sensors", not(feature = "board-physical")))]
        {
            let dht = smartpot_iot::drivers::dht22::Dht22::new(esp_hal::gpio::Flex::new(
                peripherals.GPIO15,
            ));
            _spawner.spawn(
                smartpot_iot::tasks::board_sensors::board_sensors_task(
                    adc1,
                    light,
                    ph,
                    tds,
                    soil,
                    dht,
                    SENSOR_CH.dyn_publisher().unwrap(),
                )
                .expect("Failed to spawn board_sensors_task"),
            );
        }

        #[cfg(feature = "board-physical")]
        {
            // Pines de protocolo I2C del bus principal.
            let i2c0 = peripherals.I2C0;
            let sda = peripherals.GPIO21;
            let scl = peripherals.GPIO22;

            static I2C_BUS: StaticCell<Mutex<NoopRawMutex, I2c<'static, Async>>> =
                StaticCell::new();
            // Tiempo límite por transacción: un sensor que retiene el bus ya no
            // bloquea indefinidamente a los demás.
            let i2c_config = Config::default().with_software_timeout(
                esp_hal::i2c::master::SoftwareTimeout::Transaction(
                    esp_hal::time::Duration::from_millis(25),
                ),
            );
            let i2c = I2c::new(i2c0, i2c_config)
                .unwrap()
                .with_scl(scl)
                .with_sda(sda)
                .into_async();
            let _i2c_bus = I2C_BUS.init(Mutex::new(i2c));

            _spawner.spawn(
                smartpot_iot::i2c_scanner::scan_i2c(I2cDevice::new(_i2c_bus))
                    .expect("Failed to spawn scan_i2c"),
            );

            let tca = Tca9548a::new(_i2c_bus, 0x70);
            _spawner.spawn(
                smartpot_iot::tasks::sensors::bme280_sequential_task(
                    tca,
                    SENSOR_CH.dyn_publisher().unwrap(),
                )
                .expect("Failed to spawn bme280_sequential_task"),
            );

            _spawner.spawn(
                smartpot_iot::tasks::sensors::bh1750_task(
                    I2cDevice::new(_i2c_bus),
                    SENSOR_CH.dyn_publisher().unwrap(),
                )
                .expect("Failed to spawn bh1750_task"),
            );

            _spawner.spawn(
                smartpot_iot::tasks::sensors::analog_sensors_task(
                    adc1,
                    tds,
                    ph,
                    soil,
                    SENSOR_CH.dyn_publisher().unwrap(),
                )
                .expect("Failed to spawn analog_sensors_task"),
            );
        }

        // Pantalla LCD2004 en su propio bus I2C: si falla, no afecta a los sensores.
        #[cfg(feature = "display")]
        {
            let lcd_config = Config::default().with_software_timeout(
                esp_hal::i2c::master::SoftwareTimeout::Transaction(
                    esp_hal::time::Duration::from_millis(25),
                ),
            );
            let lcd_i2c = I2c::new(peripherals.I2C1, lcd_config)
                .unwrap()
                .with_scl(peripherals.GPIO16)
                .with_sda(peripherals.GPIO17)
                .into_async();
            _spawner.spawn(
                smartpot_iot::tasks::display::display_task(lcd_i2c)
                    .expect("Failed to spawn display_task"),
            );
        }

        #[cfg(feature = "actuators")]
        {
            use esp_hal::gpio::{Level, Output, OutputConfig};

            // Las salidas nacen apagadas, también con relés activos en bajo.
            let off_level = if smartpot_iot::safety::on_level_is_high() {
                Level::Low
            } else {
                Level::High
            };

            let water = Output::new(water_pin, off_level, OutputConfig::default());
            let uv_light = Output::new(uv_light_pin, off_level, OutputConfig::default());
            let fan = Output::new(fan_pin, off_level, OutputConfig::default());
            #[cfg(feature = "board-physical")]
            let humidifier = Some(Output::new(
                humidifier_pin,
                off_level,
                OutputConfig::default(),
            ));
            #[cfg(not(feature = "board-physical"))]
            let humidifier = None;

            _spawner.spawn(
                smartpot_iot::tasks::command::catch_commands(
                    COMMAND_CH.dyn_subscriber().unwrap(),
                    ACK_CH.dyn_publisher().unwrap(),
                    water,
                    uv_light,
                    fan,
                    humidifier,
                )
                .expect("Failed to spawn catch_commands"),
            );
        }
    };

    #[allow(static_mut_refs)]
    {
        const CORE1_STACK_SIZE: usize = 16384;
        static CORE1_STACK: StaticCell<esp_hal::system::Stack<CORE1_STACK_SIZE>> =
            StaticCell::new();
        let core1_stack = CORE1_STACK.init_with(esp_hal::system::Stack::new);
        // SAFETY: la pila aún no la usa nadie; se pinta para medir su marca de agua.
        unsafe {
            smartpot_iot::health::paint_core1_stack(core1_stack.bottom(), CORE1_STACK_SIZE / 4);
        }

        start_second_core(
            peripherals.CPU_CTRL,
            sw_int.software_interrupt1,
            core1_stack,
            move || {
                info!("Starting second core (CORE1)");
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

    spawner.spawn(
        smartpot_iot::tasks::supervisor::supervisor_task(rtc)
            .expect("Failed to spawn supervisor_task"),
    );

    // Build de simulación: comandos y ganchos de prueba por la consola serie.
    #[cfg(feature = "sim")]
    {
        let rx = esp_hal::uart::UartRx::new(peripherals.UART0, esp_hal::uart::Config::default())
            .expect("Failed to configure UART0 RX")
            .with_rx(peripherals.GPIO3)
            .into_async();
        spawner.spawn(
            smartpot_iot::sim::hooks::serial_injector(
                rx,
                smartpot_iot::events::COMMAND_CH.dyn_publisher().unwrap(),
            )
            .expect("Failed to spawn serial_injector"),
        );
    }

    #[cfg(feature = "wifi")]
    {
        use embassy_net::{DhcpConfig, StackResources};
        use esp_hal::rng::Trng;
        use esp_radio::wifi::ControllerConfig;
        use smartpot_iot::tasks::time::sntp_task;
        use smartpot_iot::tasks::wifi::{net_task, wifi_connection_task};

        #[cfg(all(
            feature = "sensors",
            any(feature = "mqtt", feature = "http-api", feature = "ble")
        ))]
        use smartpot_iot::{events::telemetry::TELEMETRY_CH, tasks::telemetry::telemetry_task};

        info!("Configuring Wi-Fi network stack and radio...");
        let (wifi_controller, interfaces) =
            esp_radio::wifi::new(peripherals.WIFI, ControllerConfig::default())
                .expect("Could not initialize wifi controller");
        let wifi_interface = interfaces.station;

        static STACK_RESOURCES: StaticCell<StackResources<5>> = StaticCell::new();
        let stack_resources = STACK_RESOURCES.init_with(StackResources::<5>::new);

        let mut trng_buf = [0u8; 16];
        // Generador aleatorio de hardware del ESP32 para semillas de red y TLS
        let trng = Trng::try_new().unwrap();
        trng.read(&mut trng_buf);

        let net_seed = trng.random() as u64 | ((trng.random() as u64) << 32);
        let net_config = embassy_net::Config::dhcpv4(DhcpConfig::default());
        let (_stack, runner) =
            embassy_net::new(wifi_interface, net_config, stack_resources, net_seed);

        spawner.spawn(net_task(runner).expect("Failed to start net_task"));
        spawner.spawn(
            wifi_connection_task(
                wifi_controller,
                trng.clone(),
                DEFAULT_CONFIG.wifi_ssid,
                DEFAULT_CONFIG.wifi_password,
            )
            .expect("Failed to start wifi_connection_task"),
        );

        spawner.spawn(
            sntp_task(_stack, DEFAULT_CONFIG.sntp_server).expect("Failed to start sntp_task"),
        );

        #[cfg(all(
            feature = "sensors",
            any(feature = "mqtt", feature = "http-api", feature = "ble")
        ))]
        spawner.spawn(
            telemetry_task(
                SENSOR_CH.dyn_subscriber().unwrap(),
                TELEMETRY_CH.dyn_publisher().unwrap(),
                DEFAULT_CONFIG.telemetry_interval_secs,
            )
            .expect("Failed to start telemetry_task"),
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
            .expect("Failed to start mqtt_task"),
        );
    }

    smartpot_iot::marker!(
        "SMARTPOT-READY firmware=smartpot-iot/{} reset={}",
        env!("CARGO_PKG_VERSION"),
        reset_cause()
    );

    loop {
        core::future::pending::<()>().await;
    }
}
