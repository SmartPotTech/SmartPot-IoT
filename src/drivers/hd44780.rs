//! Driver asíncrono del LCD HD44780 (20×4) a través del expansor I2C PCF8574.
//!
//! El mapeo de pines es el habitual de los módulos I2C y el mismo que usa el
//! firmware MicroPython: P0 = RS, P1 = RW, P2 = E, P3 = retroiluminación y
//! P4..P7 = D4..D7 en modo de 4 bits.

use embassy_time::Timer;
use embedded_hal_async::i2c::I2c;

/// Dirección habitual del PCF8574 de los módulos LCD (y la de Wokwi).
pub const LCD_ADDRESS: u8 = 0x27;

const RS: u8 = 0x01;
const ENABLE: u8 = 0x04;
const BACKLIGHT: u8 = 0x08;

/// Dirección DDRAM del comienzo de cada fila en un LCD de 20×4.
const ROW_OFFSETS: [u8; 4] = [0x00, 0x40, 0x14, 0x54];

pub struct Hd44780<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C> Hd44780<I2C>
where
    I2C: I2c,
{
    /// Crea el driver sin tocar el bus; [`Self::init`] configura la pantalla.
    pub const fn new(i2c: I2C, address: u8) -> Self {
        Self { i2c, address }
    }

    /// Secuencia de arranque en modo de 4 bits del datasheet del HD44780.
    pub async fn init(&mut self) -> Result<(), I2C::Error> {
        Timer::after_millis(50).await;
        self.i2c.write(self.address, &[BACKLIGHT]).await?;
        for delay_us in [4_500, 150, 150] {
            self.write_nibble(0x30, 0).await?;
            Timer::after_micros(delay_us).await;
        }
        self.write_nibble(0x20, 0).await?;
        // 4 bits, dos líneas lógicas (las cuatro filas físicas), caracteres de 5×8.
        self.command(0x28).await?;
        // Pantalla encendida sin cursor.
        self.command(0x0C).await?;
        self.command(0x01).await?;
        Timer::after_millis(2).await;
        // Escritura de izquierda a derecha sin desplazar la pantalla.
        self.command(0x06).await
    }

    /// Carga un carácter propio en la CGRAM (posiciones 0 a 7).
    pub async fn custom_char(&mut self, location: u8, bitmap: &[u8; 8]) -> Result<(), I2C::Error> {
        self.command(0x40 | ((location & 0x07) << 3)).await?;
        for &line in bitmap {
            self.send(line, RS).await?;
        }
        Ok(())
    }

    /// Borra la pantalla y vuelve el cursor al inicio.
    pub async fn clear(&mut self) -> Result<(), I2C::Error> {
        self.command(0x01).await?;
        Timer::after_millis(2).await;
        Ok(())
    }

    /// Escribe `text` a partir de la columna `col` de la fila `row`.
    pub async fn write_at(&mut self, row: usize, col: u8, text: &str) -> Result<(), I2C::Error> {
        self.command(0x80 | (ROW_OFFSETS[row % ROW_OFFSETS.len()] + col))
            .await?;
        for ch in text.chars() {
            self.send(character_code(ch), RS).await?;
        }
        Ok(())
    }

    /// Escribe una fila completa desde la primera columna.
    pub async fn write_row(&mut self, row: usize, text: &str) -> Result<(), I2C::Error> {
        self.command(0x80 | ROW_OFFSETS[row % ROW_OFFSETS.len()])
            .await?;
        for ch in text.chars() {
            self.send(character_code(ch), RS).await?;
        }
        Ok(())
    }

    async fn command(&mut self, command: u8) -> Result<(), I2C::Error> {
        self.send(command, 0).await
    }

    async fn send(&mut self, byte: u8, mode: u8) -> Result<(), I2C::Error> {
        self.write_nibble(byte & 0xF0, mode).await?;
        self.write_nibble(byte << 4, mode).await
    }

    /// Envía los 4 bits altos de `nibble` con un pulso en la línea E.
    ///
    /// Cada flanco va en su propia transacción I2C, igual que el driver del
    /// firmware MicroPython y LiquidCrystal_I2C.
    async fn write_nibble(&mut self, nibble: u8, mode: u8) -> Result<(), I2C::Error> {
        let data = (nibble & 0xF0) | mode | BACKLIGHT;
        self.i2c.write(self.address, &[data | ENABLE]).await?;
        self.i2c.write(self.address, &[data]).await?;
        Timer::after_micros(50).await;
        Ok(())
    }
}

/// Código de la ROM A00 del HD44780: ASCII, los caracteres propios 0..7 y,
/// como en MicroPython, los códigos 0x80..0xFF de la ROM (el *spinner*).
fn character_code(ch: char) -> u8 {
    let code = ch as u32;
    if code < 0x100 { code as u8 } else { b'?' }
}
