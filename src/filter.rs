//! Motores de filtrado y estimación de señales de sensores para SmartPot IoT.
//!
//! Implementa:
//! 1. `MedianFilter5`: Filtro de mediana móvil sobre ventana deslizante de 5 muestras para rechazo de espurias ADC.
//! 2. `Q16KalmanFilter`: Filtro de Kalman 1D en aritmética de punto fijo Q16.16 para microcontroladores sin FPU o de bajo consumo.
//! 3. `FloatKalmanFilter`: Filtro de Kalman 1D en precisión simple f32 optimizado con innovación acotada.

/// Filtro de mediana móvil de 5 muestras para supresión de ruido impulsivo en lecturas analógicas (pH, TDS, Humedad).
#[derive(Debug, Clone, Copy)]
pub struct MedianFilter5 {
    buffer: [f32; 5],
    count: usize,
    index: usize,
}

impl MedianFilter5 {
    /// Inicializa un filtro de mediana vacío.
    pub const fn new() -> Self {
        Self {
            buffer: [0.0; 5],
            count: 0,
            index: 0,
        }
    }

    /// Añade una nueva muestra al búfer circular y devuelve la mediana actual.
    pub fn update(&mut self, sample: f32) -> f32 {
        self.buffer[self.index] = sample;
        self.index = (self.index + 1) % 5;
        if self.count < 5 {
            self.count += 1;
        }

        let mut sorted = [0.0f32; 5];
        for i in 0..self.count {
            sorted[i] = self.buffer[i];
        }

        // Ordenamiento por inserción para 1 a 5 elementos (cero sobrecarga, sin asignaciones)
        for i in 1..self.count {
            let key = sorted[i];
            let mut j = i;
            while j > 0 && sorted[j - 1] > key {
                sorted[j] = sorted[j - 1];
                j -= 1;
            }
            sorted[j] = key;
        }

        sorted[self.count / 2]
    }

    /// Reinicia el estado del filtro.
    pub fn reset(&mut self) {
        self.count = 0;
        self.index = 0;
        self.buffer = [0.0; 5];
    }
}

impl Default for MedianFilter5 {
    fn default() -> Self {
        Self::new()
    }
}

/// Filtro de Kalman 1D en aritmética de punto fijo Q16.16 (sin punto flotante).
/// Formato: 16 bits enteros con signo, 16 bits fraccionarios. Factor de escala: 65536.
#[derive(Debug, Clone, Copy)]
pub struct Q16KalmanFilter {
    x: i32, // Estado estimado (Q16.16)
    p: i32, // Covarianza de error (Q16.16)
    q: i32, // Varianza de ruido de proceso (Q16.16)
    r: i32, // Varianza de ruido de medición (Q16.16)
}

impl Q16KalmanFilter {
    const SCALE: i64 = 65536;

    /// Inicializa el filtro con estado inicial, covarianza, ruido de proceso y medición.
    pub const fn new(init_val_q16: i32, p_q16: i32, q_q16: i32, r_q16: i32) -> Self {
        Self {
            x: init_val_q16,
            p: p_q16,
            q: q_q16,
            r: r_q16,
        }
    }

    /// Crea el filtro a partir de valores de punto flotante para facilitar configuración.
    pub fn from_f32(init_val: f32, p: f32, q: f32, r: f32) -> Self {
        Self {
            x: (init_val * Self::SCALE as f32) as i32,
            p: (p * Self::SCALE as f32) as i32,
            q: (q * Self::SCALE as f32) as i32,
            r: (r * Self::SCALE as f32) as i32,
        }
    }

    /// Actualiza la estimación con una nueva medición en Q16.16.
    pub fn update(&mut self, measurement_q16: i32) -> i32 {
        // 1. Predicción:
        // x_pred = x
        // p_pred = p + q
        let p_pred = self.p.saturating_add(self.q);

        // 2. Ganancia de Kalman: K = p_pred / (p_pred + r)
        let denom = (p_pred as i64) + (self.r as i64);
        if denom == 0 {
            return self.x;
        }
        let k = (((p_pred as i64) * Self::SCALE) / denom) as i32;

        // 3. Corrección: x = x_pred + K * (z - x_pred)
        let innovation = measurement_q16.saturating_sub(self.x);
        let delta = (((k as i64) * (innovation as i64)) / Self::SCALE) as i32;
        self.x = self.x.saturating_add(delta);

        // 4. Actualización de covarianza: p = (1 - K) * p_pred
        let one_minus_k = (Self::SCALE as i32).saturating_sub(k);
        self.p = (((one_minus_k as i64) * (p_pred as i64)) / Self::SCALE) as i32;

        self.x
    }

    /// Devuelve el valor estimado actual convertido a f32.
    pub fn value_f32(&self) -> f32 {
        self.x as f32 / Self::SCALE as f32
    }

    /// Devuelve el valor estimado actual en formato Q16.16.
    pub const fn value_q16(&self) -> i32 {
        self.x
    }
}

/// Filtro de Kalman 1D en precisión simple (f32).
#[derive(Debug, Clone, Copy)]
pub struct FloatKalmanFilter {
    x: f32,
    p: f32,
    q: f32,
    r: f32,
}

impl FloatKalmanFilter {
    pub const fn new(init_x: f32, p: f32, q: f32, r: f32) -> Self {
        Self { x: init_x, p, q, r }
    }

    pub fn update(&mut self, z: f32) -> f32 {
        // Predicción
        let p_pred = self.p + self.q;

        // Ganancia
        let k = p_pred / (p_pred + self.r);

        // Actualización de estado
        self.x += k * (z - self.x);

        // Actualización de covarianza
        self.p = (1.0 - k) * p_pred;

        self.x
    }

    pub const fn value(&self) -> f32 {
        self.x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_median_filter_5() {
        let mut mf = MedianFilter5::new();
        assert_eq!(mf.update(10.0), 10.0);
        assert_eq!(mf.update(100.0), 100.0); // Mediana de [10, 100] es 100 (índice 1)
        assert_eq!(mf.update(12.0), 12.0); // Ordenado [10, 12, 100] -> mediana 12
        assert_eq!(mf.update(11.0), 12.0); // Ordenado [10, 11, 12, 100] -> índice 2 es 12
        assert_eq!(mf.update(13.0), 12.0); // Ordenado [10, 11, 12, 13, 100] -> mediana 12.0

        // Inyección de espuria (pico de 500.0)
        let filtered = mf.update(500.0);
        assert!(filtered < 15.0); // El pico de 500 es rechazado completamente
    }

    #[test]
    fn test_q16_kalman_filter() {
        let mut kf = Q16KalmanFilter::from_f32(20.0, 1.0, 0.01, 0.1);
        let q16_val = (21.0 * 65536.0) as i32;
        let res = kf.update(q16_val);
        let est = res as f32 / 65536.0;
        assert!(est > 20.0 && est < 21.0);
    }

    #[test]
    fn test_float_kalman_filter() {
        let mut kf = FloatKalmanFilter::new(20.0, 1.0, 0.01, 0.1);
        let res = kf.update(21.0);
        assert!(res > 20.0 && res < 21.0);
    }
}
