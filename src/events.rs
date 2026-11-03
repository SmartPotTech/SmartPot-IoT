use defmt::Format;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::pubsub::PubSubChannel;

#[cfg(feature = "sensors")]
use heapless::String;

// SENSORS
#[cfg(feature = "sensors")]
#[derive(Clone, Copy, Debug, Format)]
pub enum Measurements {
    BME280((f32, f32, f32)),
    BH1750(f32),
    PH(f32),
    TDS(f32),
}
#[cfg(feature = "sensors")]
unsafe impl Send for Measurements {}

#[cfg(feature = "sensors")]
impl Measurements {
    pub fn to_json<const N: usize>(&self, buf: &mut String<N>) {
        use core::fmt::Write;

        let _ = match self {
            Measurements::BME280((t, h, p)) => {
                write!(
                    buf,
                    "{{\"temperature\":{},\"humidity\":{},\"pressure\":{}}}",
                    t, h, p
                )
            }
            Measurements::BH1750(l) => {
                write!(buf, "{{\"lux\":{}}}", l)
            }
            Measurements::PH(p) => {
                write!(buf, "{{\"ph\":{}}}", p)
            }
            Measurements::TDS(t) => {
                write!(buf, "{{\"tds\":{}}}", t)
            }
        };
    }

    pub fn get_topic(&self) -> &str {
        match self {
            Measurements::BME280(_) => "bme280",
            Measurements::BH1750(_) => "lux",
            Measurements::PH(_) => "ph",
            Measurements::TDS(_) => "tds",
        }
    }
}

#[cfg(feature = "sensors")]
pub const SENSOR_CH_CAP: usize = 32;
#[cfg(feature = "sensors")]
pub const SENSOR_CH_PUB: usize = 4;
#[cfg(feature = "sensors")]
pub const SENSOR_CH_SUB: usize = 2;
#[cfg(feature = "sensors")]
pub static SENSOR_CH: PubSubChannel<
    CriticalSectionRawMutex,
    Measurements,
    SENSOR_CH_CAP,
    SENSOR_CH_SUB,
    SENSOR_CH_PUB,
> = PubSubChannel::new();

//
// COMMANDS / ACTUATORS
//

#[derive(Clone, Copy, Debug, Format)]
pub enum Actuators {
    WaterPump,
    Lights,
    Humidifier,
}

impl Actuators {
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "water_pump" => Ok(Self::WaterPump),
            "lights" => Ok(Self::Lights),
            "humidifier" => Ok(Self::Humidifier),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy, Debug, Format)]
pub enum Command {
    Activate(Actuators),
    Disable(Actuators),

    #[cfg(feature = "sensors")]
    Threshold(Measurements),
}

unsafe impl Send for Command {}

impl Command {
    pub fn new(action: &str, arg: &str) -> Result<Self, ()> {
        match action {
            "activate" => {
                let actuator = Actuators::from_str(arg)?;
                Ok(Command::Activate(actuator))
            }
            "disable" => {
                let actuator = Actuators::from_str(arg)?;
                Ok(Command::Disable(actuator))
            }
            _ => Err(()),
        }
    }
}

pub const COMMAND_CH_CAP: usize = 8;
pub const COMMAND_CH_PUB: usize = 1;
pub const COMMAND_CH_SUB: usize = 1;
pub static COMMAND_CH: PubSubChannel<
    CriticalSectionRawMutex,
    Command,
    COMMAND_CH_CAP,
    COMMAND_CH_SUB,
    COMMAND_CH_PUB,
> = PubSubChannel::new();

#[cfg(feature = "sensors")]
#[derive(Clone, Copy, Default, Format)]
pub struct SensorState {
    pub temperature: Option<f32>,
    pub humidity: Option<f32>,
    pub pressure: Option<f32>,
    pub lux: Option<f32>,
    pub ph: Option<f32>,
    pub tds: Option<f32>,
}

#[cfg(feature = "sensors")]
impl SensorState {
    pub fn update(&mut self, measurement: Measurements) {
        match measurement {
            Measurements::BME280((t, h, p)) => {
                self.temperature = Some(t);
                self.humidity = Some(h);
                self.pressure = Some(p);
            }

            Measurements::BH1750(l) => {
                self.lux = Some(l);
            }

            Measurements::PH(p) => {
                self.ph = Some(p);
            }

            Measurements::TDS(t) => {
                self.tds = Some(t);
            }
        }
    }

    pub fn clean(&mut self) {
        *self = Self {
            temperature: None,
            humidity: None,
            pressure: None,
            lux: None,
            ph: None,
            tds: None,
        };
    }
}

#[cfg(feature = "sensors")]
impl SensorState {
    pub fn to_json<const N: usize>(&self, buf: &mut heapless::String<N>) {
        use core::fmt::Write;

        fn write_opt<const N: usize>(buf: &mut heapless::String<N>, value: Option<f32>) {
            match value {
                Some(v) => {
                    let _ = write!(buf, "{}", v);
                }
                None => {
                    let _ = write!(buf, "null");
                }
            }
        }

        let _ = write!(buf, "{{");

        //
        // LUX
        //
        match self.lux {
            Some(v) => {
                let _ = write!(buf, "\"lux\":{},", v);
            }
            None => {
                let _ = write!(buf, "\"lux\":null,");
            }
        }

        //
        // BME280
        //

        let _ = write!(buf, "\"temperature\":");
        write_opt(buf, self.temperature);

        let _ = write!(buf, ",\"humidity\":");
        write_opt(buf, self.humidity);

        let _ = write!(buf, ",\"pressure\":");
        write_opt(buf, self.pressure);
        let _ = write!(buf, ",");
        //
        // TDS
        //
        match self.tds {
            Some(v) => {
                let _ = write!(buf, "\"tds\":{},", v);
            }
            None => {
                let _ = write!(buf, "\"tds\":null,");
            }
        }

        //
        // PH
        //
        match self.ph {
            Some(v) => {
                let _ = write!(buf, "\"ph\":{}", v);
            }
            None => {
                let _ = write!(buf, "\"ph\":null");
            }
        }

        let _ = write!(buf, "}}");
    }
}

// Events to send
#[cfg(all(
    feature = "sensors",
    any(feature = "mqtt", feature = "http-api", feature = "ble")
))]
pub mod telemetry {
    use crate::events::{Measurements, SensorState};
    use defmt::Format;
    use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, pubsub::PubSubChannel};

    #[derive(Copy, Clone, Format)]
    pub enum TelemetryMessage {
        SendOne(Measurements),
        SendAll(SensorState),
    }

    const fn enable_features() -> usize {
        let mut counter = 0;

        if cfg!(feature = "mqtt") {
            counter += 1;
        }

        if cfg!(feature = "http-api") {
            counter += 1;
        }

        if cfg!(feature = "ble") {
            counter += 1;
        }

        counter
    }

    pub const TELEMETRY_CH_CAP: usize = 8;
    pub const TELEMETRY_CH_SUB: usize = enable_features();
    pub const TELEMETRY_CH_PUB: usize = 1;

    pub static TELEMETRY_CH: PubSubChannel<
        CriticalSectionRawMutex,
        TelemetryMessage,
        TELEMETRY_CH_CAP,
        TELEMETRY_CH_SUB,
        TELEMETRY_CH_PUB,
    > = PubSubChannel::new();
}

/* FUTURE PLANS
pub enum SystemServices {
    WifiUp(bool),
    BLE(bool)
}
*/
