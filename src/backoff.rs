//! Espera exponencial con dispersión aleatoria para los reintentos de red.
//!
//! Si el broker o el punto de acceso se reinician, todos los dispositivos
//! reintentan a la vez. Con una espera fija de 5 s la flota entera reconecta en
//! el mismo instante; aquí cada intento duplica la espera hasta `max_ms` y se
//! elige un valor al azar en la mitad superior del intervalo. El módulo es puro
//! para poder probarlo en el host; el número aleatorio lo aporta el llamador.

/// Estado de una secuencia de reintentos.
#[derive(Clone, Copy, Debug)]
pub struct Backoff {
    base_ms: u32,
    max_ms: u32,
    attempt: u32,
}

impl Backoff {
    /// Crea una secuencia que empieza en `base_ms` y nunca supera `max_ms`.
    pub const fn new(base_ms: u32, max_ms: u32) -> Self {
        Self {
            base_ms,
            max_ms,
            attempt: 0,
        }
    }

    /// Vuelve al primer intervalo tras una conexión correcta.
    pub fn reset(&mut self) {
        self.attempt = 0;
    }

    /// Número de intentos fallidos consecutivos.
    pub const fn attempt(&self) -> u32 {
        self.attempt
    }

    /// Devuelve la próxima espera en milisegundos y avanza la secuencia.
    ///
    /// La espera cae en `[techo / 2, techo]`, donde el techo es
    /// `base_ms * 2^intentos` limitado a `max_ms`.
    pub fn next_delay_ms(&mut self, random: u32) -> u32 {
        let shift = self.attempt.min(16);
        let ceiling = self
            .base_ms
            .saturating_mul(1u32 << shift)
            .min(self.max_ms)
            .max(1);
        self.attempt = self.attempt.saturating_add(1);

        let floor = ceiling / 2;
        floor + random % (ceiling - floor + 1)
    }
}
