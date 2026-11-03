//! Formateo y serialización de telemetría según el contrato v1 de SmartPot.
//!
//! Campos físicos validados por la plataforma:
//! - `temperature`: °C con 1 decimal
//! - `humidity`: % con 1 decimal
//! - `brightness`: lux redondeado a entero
//! - `ph`: escala 0..=14 con 2 decimales
//! - `tds`: ppm redondeado a entero
//! - `atmosphere`: hPa con 1 decimal
//! - `soilMoisture`: % con 1 decimal

use core::fmt::Write;
use defmt::Format;
use heapless::String;

#[derive(Clone, Copy, Default, Debug, Format)]
pub struct SensorState {
    pub temperature: Option<f32>,
    pub humidity: Option<f32>,
    pub brightness: Option<f32>,
    pub ph: Option<f32>,
    pub tds: Option<f32>,
    pub atmosphere: Option<f32>,
    pub soil_moisture: Option<f32>,
}

impl SensorState {
    pub fn update_bme280(&mut self, temp: f32, hum: f32, press_hpa: f32) {
        self.temperature = Some(temp);
        self.humidity = Some(hum);
        self.atmosphere = Some(press_hpa);
    }

    pub fn update_lux(&mut self, lux: f32) {
        self.brightness = Some(lux);
    }

    pub fn update_ph(&mut self, ph: f32) {
        self.ph = Some(ph);
    }

    pub fn update_tds(&mut self, tds: f32) {
        self.tds = Some(tds);
    }

    pub fn update_soil_moisture(&mut self, moisture: f32) {
        self.soil_moisture = Some(moisture);
    }

    pub fn clean(&mut self) {
        *self = Self::default();
    }

    pub fn is_empty(&self) -> bool {
        self.temperature.is_none()
            && self.humidity.is_none()
            && self.brightness.is_none()
            && self.ph.is_none()
            && self.tds.is_none()
            && self.atmosphere.is_none()
            && self.soil_moisture.is_none()
    }

    /// Serializa el estado de sensores a JSON compacto omitiendo campos ausentes.
    pub fn to_json<const N: usize>(&self, buf: &mut String<N>) {
        buf.clear();
        let _ = write!(buf, "{{");
        let mut first = true;

        if let Some(t) = self.temperature {
            let _ = write!(buf, "\"temperature\":{:.1}", t);
            first = false;
        }
        if let Some(h) = self.humidity {
            if !first {
                let _ = write!(buf, ",");
            }
            let _ = write!(buf, "\"humidity\":{:.1}", h);
            first = false;
        }
        if let Some(b) = self.brightness {
            if !first {
                let _ = write!(buf, ",");
            }
            let _ = write!(buf, "\"brightness\":{:.0}", b);
            first = false;
        }
        if let Some(p) = self.ph {
            if !first {
                let _ = write!(buf, ",");
            }
            let _ = write!(buf, "\"ph\":{:.2}", p);
            first = false;
        }
        if let Some(t) = self.tds {
            if !first {
                let _ = write!(buf, ",");
            }
            let _ = write!(buf, "\"tds\":{:.0}", t);
            first = false;
        }
        if let Some(atm) = self.atmosphere {
            if !first {
                let _ = write!(buf, ",");
            }
            let _ = write!(buf, "\"atmosphere\":{:.1}", atm);
            first = false;
        }
        if let Some(sm) = self.soil_moisture {
            if !first {
                let _ = write!(buf, ",");
            }
            let _ = write!(buf, "\"soilMoisture\":{:.1}", sm);
        }

        let _ = write!(buf, "}}");
    }
}
