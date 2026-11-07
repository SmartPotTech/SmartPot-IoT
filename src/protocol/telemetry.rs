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
//!
//! Las lecturas no finitas o fuera de los rangos de la API se omiten para que
//! una sola medida inválida no invalide el paquete JSON completo.

use core::fmt::Write;
use defmt::Format;
use heapless::String;

use crate::limits;

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

    /// Serializa el estado de sensores a JSON compacto omitiendo campos ausentes
    /// o fuera de los rangos aceptados por la API.
    pub fn to_json<const N: usize>(&self, buf: &mut String<N>) {
        buf.clear();
        let _ = write!(buf, "{{");
        let mut first = true;

        if let Some(t) = valid(self.temperature, limits::TEMPERATURE) {
            let _ = write!(buf, "\"temperature\":{:.1}", t);
            first = false;
        }
        if let Some(h) = valid(self.humidity, limits::HUMIDITY) {
            if !first {
                let _ = write!(buf, ",");
            }
            let _ = write!(buf, "\"humidity\":{:.1}", h);
            first = false;
        }
        if let Some(b) = valid(self.brightness, limits::BRIGHTNESS) {
            if !first {
                let _ = write!(buf, ",");
            }
            let _ = write!(buf, "\"brightness\":{:.0}", b);
            first = false;
        }
        if let Some(p) = valid(self.ph, limits::PH) {
            if !first {
                let _ = write!(buf, ",");
            }
            let _ = write!(buf, "\"ph\":{:.2}", p);
            first = false;
        }
        if let Some(t) = valid(self.tds, limits::TDS) {
            if !first {
                let _ = write!(buf, ",");
            }
            let _ = write!(buf, "\"tds\":{:.0}", t);
            first = false;
        }
        if let Some(atm) = valid(self.atmosphere, limits::ATMOSPHERE) {
            if !first {
                let _ = write!(buf, ",");
            }
            let _ = write!(buf, "\"atmosphere\":{:.1}", atm);
            first = false;
        }
        if let Some(sm) = valid(self.soil_moisture, limits::SOIL_MOISTURE) {
            if !first {
                let _ = write!(buf, ",");
            }
            let _ = write!(buf, "\"soilMoisture\":{:.1}", sm);
        }

        let _ = write!(buf, "}}");
    }
}

/// Campos del estado de sensores, en el orden de `FreshState::updated_ms`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Temperature,
    Humidity,
    Brightness,
    Ph,
    Tds,
    Atmosphere,
    SoilMoisture,
}

/// Estado de sensores con la hora de la última lectura de cada campo.
///
/// Un sensor que deja de responder ya no repite su último valor para siempre:
/// al tomar la instantánea, los campos sin lectura reciente se omiten.
#[derive(Clone, Copy, Debug, Default)]
pub struct FreshState {
    state: SensorState,
    updated_ms: [Option<u64>; 7],
}

impl FreshState {
    pub fn set(&mut self, field: Field, value: f32, now_ms: u64) {
        let slot = match field {
            Field::Temperature => &mut self.state.temperature,
            Field::Humidity => &mut self.state.humidity,
            Field::Brightness => &mut self.state.brightness,
            Field::Ph => &mut self.state.ph,
            Field::Tds => &mut self.state.tds,
            Field::Atmosphere => &mut self.state.atmosphere,
            Field::SoilMoisture => &mut self.state.soil_moisture,
        };
        *slot = Some(value);
        self.updated_ms[field as usize] = Some(now_ms);
    }

    /// Estado con solo los campos leídos en los últimos `ttl_ms` milisegundos.
    pub fn snapshot(&self, now_ms: u64, ttl_ms: u64) -> SensorState {
        let fresh = |field: Field, value: Option<f32>| {
            let updated = self.updated_ms[field as usize]?;
            if now_ms.saturating_sub(updated) > ttl_ms {
                None
            } else {
                value
            }
        };
        SensorState {
            temperature: fresh(Field::Temperature, self.state.temperature),
            humidity: fresh(Field::Humidity, self.state.humidity),
            brightness: fresh(Field::Brightness, self.state.brightness),
            ph: fresh(Field::Ph, self.state.ph),
            tds: fresh(Field::Tds, self.state.tds),
            atmosphere: fresh(Field::Atmosphere, self.state.atmosphere),
            soil_moisture: fresh(Field::SoilMoisture, self.state.soil_moisture),
        }
    }
}

fn valid(value: Option<f32>, range: limits::ApiRange) -> Option<f32> {
    value.filter(|v| range.contains(*v))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(state: &SensorState) -> String<255> {
        let mut buf = String::new();
        state.to_json(&mut buf);
        buf
    }

    #[test]
    fn omits_non_finite_and_out_of_range_values() {
        let state = SensorState {
            temperature: Some(f32::NAN),
            humidity: Some(150.0),
            brightness: Some(-1.0),
            ph: Some(14.5),
            tds: Some(10_500.0),
            atmosphere: Some(1_200.0),
            soil_moisture: Some(f32::INFINITY),
        };
        assert_eq!(render(&state).as_str(), "{}");
    }

    #[test]
    fn keeps_valid_values_and_omits_only_the_invalid_field() {
        let state = SensorState {
            temperature: Some(21.5),
            humidity: Some(101.0),
            brightness: Some(12_345.0),
            ph: Some(6.4),
            tds: None,
            atmosphere: Some(1_013.2),
            soil_moisture: Some(48.0),
        };
        let json = render(&state);
        assert!(json.contains("\"temperature\":21.5"));
        assert!(!json.contains("humidity"));
        assert!(json.contains("\"brightness\":12345"));
        assert!(json.contains("\"ph\":6.40"));
        assert!(json.contains("\"atmosphere\":1013.2"));
        assert!(json.contains("\"soilMoisture\":48.0"));
    }

    #[test]
    fn exact_api_boundaries_are_kept() {
        let state = SensorState {
            temperature: Some(60.0),
            humidity: Some(0.0),
            brightness: Some(200_000.0),
            ph: Some(14.0),
            tds: Some(10_000.0),
            atmosphere: Some(300.0),
            soil_moisture: Some(100.0),
        };
        let json = render(&state);
        assert!(json.contains("\"temperature\":60.0"));
        assert!(json.contains("\"humidity\":0.0"));
        assert!(json.contains("\"brightness\":200000"));
        assert!(json.contains("\"ph\":14.00"));
        assert!(json.contains("\"tds\":10000"));
        assert!(json.contains("\"atmosphere\":300.0"));
        assert!(json.contains("\"soilMoisture\":100.0"));
    }
}
