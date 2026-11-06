//! Rangos físicos aceptados por la API de SmartPot.
//!
//! El códec compacto v2 y el JSON del contrato v1 comparten estos límites para
//! que una lectura fuera de rango nunca envenene el paquete completo: el campo
//! se omite (JSON) o se deja con su bit de presencia en cero (binario).

/// Rango físico válido de un sensor, en sus unidades de la API.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ApiRange {
    pub min: f32,
    pub max: f32,
}

impl ApiRange {
    /// Indica si el valor es finito y cae dentro del rango, extremos incluidos.
    pub fn contains(&self, value: f32) -> bool {
        value.is_finite() && value >= self.min && value <= self.max
    }

    /// Indica si el valor ya escalado y redondeado cae dentro del rango escalado.
    ///
    /// Se usa en el códec compacto: el bit de presencia solo se activa cuando
    /// el entero que se va a transmitir representa una lectura válida.
    pub fn contains_scaled(&self, scaled: f32, scale: f32) -> bool {
        scaled.is_finite() && scaled >= self.min * scale && scaled <= self.max * scale
    }
}

/// Temperatura del aire en grados Celsius.
pub const TEMPERATURE: ApiRange = ApiRange {
    min: -20.0,
    max: 60.0,
};

/// Humedad relativa del aire en porcentaje.
pub const HUMIDITY: ApiRange = ApiRange {
    min: 0.0,
    max: 100.0,
};

/// Iluminancia en lux.
pub const BRIGHTNESS: ApiRange = ApiRange {
    min: 0.0,
    max: 200_000.0,
};

/// pH de la solución nutritiva.
pub const PH: ApiRange = ApiRange {
    min: 0.0,
    max: 14.0,
};

/// Sólidos disueltos totales en partes por millón.
pub const TDS: ApiRange = ApiRange {
    min: 0.0,
    max: 10_000.0,
};

/// Presión atmosférica en hectopascales.
pub const ATMOSPHERE: ApiRange = ApiRange {
    min: 300.0,
    max: 1_100.0,
};

/// Humedad del sustrato en porcentaje.
pub const SOIL_MOISTURE: ApiRange = ApiRange {
    min: 0.0,
    max: 100.0,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_boundaries_and_rejects_outside() {
        assert!(TEMPERATURE.contains(-20.0));
        assert!(TEMPERATURE.contains(60.0));
        assert!(!TEMPERATURE.contains(-20.01));
        assert!(!TEMPERATURE.contains(60.01));

        assert!(HUMIDITY.contains(0.0));
        assert!(HUMIDITY.contains(100.0));
        assert!(!HUMIDITY.contains(100.01));

        assert!(BRIGHTNESS.contains(200_000.0));
        assert!(!BRIGHTNESS.contains(200_001.0));

        assert!(PH.contains(14.0));
        assert!(!PH.contains(14.01));

        assert!(TDS.contains(10_000.0));
        assert!(!TDS.contains(10_001.0));

        assert!(ATMOSPHERE.contains(300.0));
        assert!(ATMOSPHERE.contains(1_100.0));
        assert!(!ATMOSPHERE.contains(299.9));
        assert!(!ATMOSPHERE.contains(1_100.1));

        assert!(SOIL_MOISTURE.contains(0.0));
        assert!(SOIL_MOISTURE.contains(100.0));
        assert!(!SOIL_MOISTURE.contains(-0.01));
    }

    #[test]
    fn rejects_non_finite_values() {
        assert!(!TEMPERATURE.contains(f32::NAN));
        assert!(!HUMIDITY.contains(f32::INFINITY));
        assert!(!PH.contains(f32::NEG_INFINITY));
    }

    #[test]
    fn scaled_range_uses_the_rounded_integer_space() {
        // 60.004 °C redondea a 6000 centésimas, que sí es un valor de API válido.
        assert!(TEMPERATURE.contains_scaled(6000.0, 100.0));
        assert!(!TEMPERATURE.contains_scaled(6001.0, 100.0));
        assert!(ATMOSPHERE.contains_scaled(3000.0, 10.0));
        assert!(!ATMOSPHERE.contains_scaled(2999.0, 10.0));
    }
}
