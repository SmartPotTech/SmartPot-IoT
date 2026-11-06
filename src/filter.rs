//! Motores de filtrado y estimación de señales de sensores para SmartPot IoT.
//!
//! Implementa:
//! 1. `MedianFilter5`: Filtro de mediana móvil sobre ventana deslizante de 5 muestras para rechazo de espurias ADC.
//! 2. `Q16KalmanFilter`: Filtro de Kalman 1D con estado en punto fijo Q16.16; la raíz cuadrada de la puerta de innovación se calcula con `libm`.
//! 3. `FloatKalmanFilter`: Filtro de Kalman 1D en precisión simple f32 optimizado con innovación acotada.
//! 4. `SensorCascade`: Cascada mediana + Kalman Q16.16 con inicialización perezosa, pensada para cada canal físico.

use libm::sqrtf;

/// Convierte un valor de punto flotante a Q16.16 saturando en lugar de truncar.
#[inline]
fn f32_to_q16(value: f32) -> i32 {
    let scaled = value * Q16KalmanFilter::SCALE as f32;
    if scaled.is_nan() {
        0
    } else if scaled >= i32::MAX as f32 {
        i32::MAX
    } else if scaled <= i32::MIN as f32 {
        i32::MIN
    } else {
        scaled as i32
    }
}

/// Convierte un entero de 64 bits a 32 bits saturando, sin truncar.
#[inline]
fn clamp_i64_to_i32(value: i64) -> i32 {
    value.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

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

    /// Inicializa el filtro con las cinco celdas sembradas con la primera muestra.
    ///
    /// Sembrar la ventana completa evita que una espuria posterior al arranque
    /// domine la mediana durante varios ciclos.
    pub const fn seeded(first: f32) -> Self {
        Self {
            buffer: [first; 5],
            count: 5,
            index: 0,
        }
    }

    /// Añade una nueva muestra al búfer circular y devuelve la mediana actual.
    pub fn update(&mut self, sample: f32) -> f32 {
        // La primera muestra siembra las cinco celdas: así el arranque no deja
        // cuatro ceros que falseen la mediana ni permite que un pico aislado
        // posterior desplace el resultado.
        if self.count == 0 {
            *self = Self::seeded(sample);
            return sample;
        }

        self.buffer[self.index] = sample;
        self.index = (self.index + 1) % 5;

        let mut sorted = self.buffer;
        // Ordenamiento por inserción para 5 elementos (sin asignaciones dinámicas)
        for i in 1..5 {
            let key = sorted[i];
            let mut j = i;
            while j > 0 && sorted[j - 1] > key {
                sorted[j] = sorted[j - 1];
                j -= 1;
            }
            sorted[j] = key;
        }

        sorted[2]
    }

    /// Reinicia el estado del filtro.
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

impl Default for MedianFilter5 {
    fn default() -> Self {
        Self::new()
    }
}

/// Filtro de Kalman 1D con estado en aritmética de punto fijo Q16.16.
/// Formato: 16 bits enteros con signo, 16 bits fraccionarios. Factor de escala: 65536.
/// La puerta de innovación usa `libm::sqrtf` para obtener la desviación típica.
#[derive(Debug, Clone, Copy)]
pub struct Q16KalmanFilter {
    x: i32, // Estado estimado (Q16.16)
    p: i32, // Covarianza de error (Q16.16)
    q: i32, // Varianza de ruido de proceso (Q16.16)
    r: i32, // Varianza de ruido de medición (Q16.16)
}

impl Q16KalmanFilter {
    pub const SCALE: i64 = 65536;

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
            x: f32_to_q16(init_val),
            p: f32_to_q16(p),
            q: f32_to_q16(q),
            r: f32_to_q16(r),
        }
    }

    /// Actualiza la estimación con una nueva medición en Q16.16.
    pub fn update(&mut self, measurement_q16: i32) -> i32 {
        // 1. Predicción: x_pred = x, p_pred = p + q.
        let p_pred = self.p.saturating_add(self.q).max(0);

        // 2. Puerta de innovación de 3 sigma basada en la covarianza de la
        //    innovación (p_pred + r). Si la medición es un pico, se descarta
        //    el estado medido pero se expande p para que un cambio real
        //    acabe superando la puerta.
        let innovation = measurement_q16.saturating_sub(self.x);
        let variance = p_pred.saturating_add(self.r).max(0);
        let sigma = sqrtf(variance as f32 / Self::SCALE as f32) * Self::SCALE as f32;
        if (innovation as f32).abs() > 3.0 * sigma {
            self.p = p_pred;
            return self.x;
        }

        // 3. Ganancia de Kalman: K = p_pred / (p_pred + r)
        let denom = (p_pred as i64) + (self.r as i64);
        if denom <= 0 {
            self.p = p_pred;
            return self.x;
        }
        let k = clamp_i64_to_i32(((p_pred as i64) * Self::SCALE) / denom);

        // 4. Corrección: x = x_pred + K * (z - x_pred)
        let delta = clamp_i64_to_i32(((k as i64) * (innovation as i64)) / Self::SCALE);
        self.x = self.x.saturating_add(delta);

        // 5. Actualización de covarianza: p = (1 - K) * p_pred
        let one_minus_k = (Self::SCALE as i32).saturating_sub(k);
        self.p = clamp_i64_to_i32(((one_minus_k as i64) * (p_pred as i64)) / Self::SCALE).max(0);

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

/// Filtro de Kalman 1D en precisión simple (f32) con puerta de innovación de 3 sigma.
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
        let p_pred = (self.p + self.q).max(0.0);

        // Puerta de innovación: 3 sigma sobre la covarianza (p_pred + r).
        let variance = (p_pred + self.r).max(0.0);
        let innovation = z - self.x;
        if !innovation.is_finite() || innovation.abs() > 3.0 * sqrtf(variance) {
            // Se conserva la estimación y se expande la covarianza con q.
            self.p = p_pred;
            return self.x;
        }

        let denom = p_pred + self.r;
        if !denom.is_finite() || denom <= 0.0 {
            self.p = p_pred;
            return self.x;
        }

        // Ganancia
        let k = p_pred / denom;

        // Actualización de estado
        self.x += k * innovation;

        // Actualización de covarianza
        self.p = ((1.0 - k) * p_pred).max(0.0);

        self.x
    }

    pub const fn value(&self) -> f32 {
        self.x
    }
}

/// Cascada de filtrado por canal físico: mediana de 5 muestras seguida de Kalman Q16.16.
///
/// El primer valor inicializa ambos filtros, de modo que no hace falta conocer
/// la escala del sensor de antemano. `q` y `r` permiten ajustar cada canal.
#[derive(Debug, Clone, Copy)]
pub struct SensorCascade {
    median: MedianFilter5,
    kalman: Option<Q16KalmanFilter>,
    q: f32,
    r: f32,
}

impl SensorCascade {
    /// Crea una cascada con el ruido de proceso (`q`) y de medición (`r`) indicados.
    pub const fn new(q: f32, r: f32) -> Self {
        Self {
            median: MedianFilter5::new(),
            kalman: None,
            q,
            r,
        }
    }

    /// Añade una muestra y devuelve el valor filtrado.
    pub fn update(&mut self, sample: f32) -> f32 {
        let median_sample = self.median.update(sample);

        match self.kalman.as_mut() {
            Some(filter) => {
                filter.update(f32_to_q16(median_sample));
                filter.value_f32()
            }
            None => {
                // Inicialización perezosa con la primera muestra disponible.
                self.kalman = Some(Q16KalmanFilter::from_f32(
                    median_sample,
                    1.0,
                    self.q,
                    self.r,
                ));
                median_sample
            }
        }
    }

    /// Devuelve la última estimación, o 0.0 si aún no se ha recibido ninguna muestra.
    pub fn value(&self) -> f32 {
        self.kalman.map_or(0.0, |filter| filter.value_f32())
    }

    /// Reinicia la cascada para un nuevo arranque del canal.
    pub fn reset(&mut self) {
        *self = Self::new(self.q, self.r);
    }
}

impl Default for SensorCascade {
    fn default() -> Self {
        Self::new(0.01, 0.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_median_filter_5_seeds_window() {
        let mut mf = MedianFilter5::new();
        // La primera muestra siembra toda la ventana.
        assert_eq!(mf.update(10.0), 10.0);
        // Un pico posterior solo ocupa una celda de la ventana.
        assert_eq!(mf.update(100.0), 10.0);
        assert_eq!(mf.update(12.0), 10.0);
        assert_eq!(mf.update(11.0), 11.0);
        assert_eq!(mf.update(13.0), 12.0);
    }

    #[test]
    fn test_median_filter_rejects_startup_spike() {
        let mut mf = MedianFilter5::seeded(25.0);
        assert_eq!(mf.update(5000.0), 25.0);
        assert_eq!(mf.update(5000.0), 25.0);
    }

    #[test]
    fn test_q16_kalman_filter() {
        let mut kf = Q16KalmanFilter::from_f32(20.0, 1.0, 0.01, 0.1);
        let q16_val = f32_to_q16(21.0);
        let res = kf.update(q16_val);
        let est = res as f32 / Q16KalmanFilter::SCALE as f32;
        assert!(est > 20.0 && est < 21.0);
    }

    #[test]
    fn test_q16_kalman_rejects_spikes_after_warmup() {
        let mut kf = Q16KalmanFilter::from_f32(25.0, 1.0, 0.01, 0.1);
        for _ in 0..20 {
            kf.update(f32_to_q16(25.0));
        }

        for _ in 0..3 {
            let estimate = kf.update(f32_to_q16(5000.0)) as f32 / Q16KalmanFilter::SCALE as f32;
            assert!((estimate - 25.0).abs() < 0.5);
        }
    }

    #[test]
    fn test_q16_kalman_step_eventually_adapts() {
        let mut kf = Q16KalmanFilter::from_f32(0.0, 1.0, 10.0, 0.1);
        let target = 100.0;

        for _ in 0..2_000 {
            kf.update(f32_to_q16(target));
        }

        let estimate = kf.value_f32();
        assert!((estimate - target).abs() < 5.0);
    }

    #[test]
    fn test_q16_kalman_saturation_does_not_panic() {
        let mut kf = Q16KalmanFilter::from_f32(f32::MAX, f32::MAX, f32::MAX, f32::MAX);
        let _ = kf.update(i32::MAX);
        let _ = kf.update(i32::MIN);

        let mut cascade = SensorCascade::new(f32::MAX, f32::MAX);
        let _ = cascade.update(f32::MAX);
        let _ = cascade.update(f32::MIN);
    }

    #[test]
    fn test_float_kalman_filter() {
        let mut kf = FloatKalmanFilter::new(20.0, 1.0, 0.01, 0.1);
        let res = kf.update(21.0);
        assert!(res > 20.0 && res < 21.0);
    }

    #[test]
    fn test_float_kalman_rejects_spike() {
        let mut kf = FloatKalmanFilter::new(20.0, 1.0, 0.01, 0.1);
        for _ in 0..20 {
            kf.update(20.0);
        }

        assert!((kf.update(5000.0) - 20.0).abs() < 0.5);
    }

    #[test]
    fn test_sensor_cascade_startup_and_spikes() {
        let mut cascade = SensorCascade::new(0.01, 0.1);

        assert_eq!(cascade.update(25.0), 25.0);
        for _ in 0..3 {
            assert!((cascade.update(5000.0) - 25.0).abs() < 0.5);
        }
    }

    #[test]
    fn test_sensor_cascade_step_response() {
        let mut cascade = SensorCascade::new(10.0, 0.1);
        for _ in 0..20 {
            cascade.update(20.0);
        }

        for _ in 0..2_000 {
            cascade.update(120.0);
        }

        assert!((cascade.value() - 120.0).abs() < 5.0);
    }
}
