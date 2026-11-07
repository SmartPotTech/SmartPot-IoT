//! Driver para el multiplexor I2C TCA9548A.
//!
//! Cada `TcaChannel` comparte el mismo mutex del bus y selecciona su canal
//! dentro de la misma sección crítica que la operación del dispositivo. Así la
//! selección del canal es atómica respecto a `write`, `write_read` y
//! `transaction`, incluso si varias tareas usan canales distintos.

use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use embedded_hal_async::i2c::{ErrorType, I2c as I2cAsync, Operation, SevenBitAddress};

/// Multiplexor TCA9548A sobre un bus compartido.
pub struct Tca9548a<BUS: 'static> {
    bus: &'static Mutex<NoopRawMutex, BUS>,
    addr: u8,
}

impl<BUS: 'static> Tca9548a<BUS> {
    pub fn new(bus: &'static Mutex<NoopRawMutex, BUS>, addr: u8) -> Self {
        Self { bus, addr }
    }

    /// Divide el multiplexor en ocho canales que comparten el mismo bus.
    pub fn split(self) -> [TcaChannel<BUS>; 8] {
        core::array::from_fn(|channel| TcaChannel::new(self.bus, self.addr, channel as u8))
    }
}

/// Canal lógico de un TCA9548A.
pub struct TcaChannel<BUS: 'static> {
    bus: &'static Mutex<NoopRawMutex, BUS>,
    tca_addr: u8,
    channel: u8,
}

// Copiar un canal solo duplica la referencia al bus compartido: permite volver
// a crear el driver de un sensor que se reconecta.
impl<BUS: 'static> Clone for TcaChannel<BUS> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<BUS: 'static> Copy for TcaChannel<BUS> {}

impl<BUS: 'static> TcaChannel<BUS> {
    pub fn new(bus: &'static Mutex<NoopRawMutex, BUS>, tca_addr: u8, channel: u8) -> Self {
        Self {
            bus,
            tca_addr,
            channel,
        }
    }

    pub fn get_channel(&self) -> u8 {
        self.channel
    }
}

impl<BUS: 'static> I2cAsync for TcaChannel<BUS>
where
    BUS: I2cAsync,
{
    async fn write(&mut self, address: SevenBitAddress, write: &[u8]) -> Result<(), Self::Error> {
        // Un único bloqueo cubre la selección y la escritura del dispositivo.
        let tca_addr = self.tca_addr;
        let channel_mask = 1u8 << self.channel;
        let mut bus = self.bus.lock().await;
        bus.write(tca_addr, &[channel_mask]).await?;
        bus.write(address, write).await
    }

    async fn write_read(
        &mut self,
        address: SevenBitAddress,
        write: &[u8],
        read: &mut [u8],
    ) -> Result<(), Self::Error> {
        // Un único bloqueo cubre la selección y la lectura del dispositivo.
        let tca_addr = self.tca_addr;
        let channel_mask = 1u8 << self.channel;
        let mut bus = self.bus.lock().await;
        bus.write(tca_addr, &[channel_mask]).await?;
        bus.write_read(address, write, read).await
    }

    async fn transaction(
        &mut self,
        address: SevenBitAddress,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Self::Error> {
        // La selección del canal y toda la transacción son indivisibles.
        let tca_addr = self.tca_addr;
        let channel_mask = 1u8 << self.channel;
        let mut bus = self.bus.lock().await;
        bus.write(tca_addr, &[channel_mask]).await?;
        bus.transaction(address, operations).await
    }
}

impl<BUS> ErrorType for TcaChannel<BUS>
where
    BUS: 'static + ErrorType,
{
    type Error = BUS::Error;
}
