use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use embedded_hal_async::i2c::{ErrorType, I2c as I2cAsync, Operation, SevenBitAddress};

// Pendiente: permitir múltiples dispositivos en el mismo canal

pub struct Tca9548a<BUS: 'static> {
    bus: &'static Mutex<NoopRawMutex, BUS>,
    addr: u8,
}

impl<BUS> Tca9548a<BUS>
where
    BUS:,
{
    pub fn new(bus: &'static Mutex<NoopRawMutex, BUS>, addr: u8) -> Self {
        Self { bus, addr }
    }

    pub fn split(self) -> [TcaChannel<I2cDevice<'static, NoopRawMutex, BUS>>; 8] {
        [
            TcaChannel::new(I2cDevice::new(self.bus), self.addr, 0),
            TcaChannel::new(I2cDevice::new(self.bus), self.addr, 1),
            TcaChannel::new(I2cDevice::new(self.bus), self.addr, 2),
            TcaChannel::new(I2cDevice::new(self.bus), self.addr, 3),
            TcaChannel::new(I2cDevice::new(self.bus), self.addr, 4),
            TcaChannel::new(I2cDevice::new(self.bus), self.addr, 5),
            TcaChannel::new(I2cDevice::new(self.bus), self.addr, 6),
            TcaChannel::new(I2cDevice::new(self.bus), self.addr, 7),
        ]
    }
}

pub struct TcaChannel<I2C> {
    i2c: I2C,
    tca_addr: u8,
    channel: u8,
}

impl<I2C> TcaChannel<I2C> {
    pub fn new(i2c: I2C, tca_addr: u8, channel: u8) -> Self {
        Self {
            i2c,
            tca_addr,
            channel,
        }
    }

    pub fn get_channel(&self) -> u8 {
        self.channel
    }
}

impl<I2C> I2cAsync for TcaChannel<I2C>
where
    I2C: I2cAsync,
{
    async fn write(&mut self, address: SevenBitAddress, write: &[u8]) -> Result<(), Self::Error> {
        self.i2c.write(self.tca_addr, &[1 << self.channel]).await?;
        self.i2c.write(address, write).await?;
        Ok(())
    }

    async fn write_read(
        &mut self,
        address: SevenBitAddress,
        write: &[u8],
        read: &mut [u8],
    ) -> Result<(), Self::Error> {
        self.i2c.write(self.tca_addr, &[1 << self.channel]).await?;
        self.i2c.write_read(address, write, read).await?;
        Ok(())
    }

    async fn transaction(
        &mut self,
        address: SevenBitAddress,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Self::Error> {
        // 1. Seleccionar canal UNA sola vez
        self.i2c.write(self.tca_addr, &[1 << self.channel]).await?;

        // 2. Ejecutar toda la transacción
        self.i2c.transaction(address, operations).await?;
        Ok(())
    }
}

impl<I2C> ErrorType for TcaChannel<I2C>
where
    I2C: ErrorType,
{
    type Error = I2C::Error;
}
