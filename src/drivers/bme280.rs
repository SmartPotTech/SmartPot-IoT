use defmt::Format;
use embassy_time::{Duration, Timer};
use embedded_hal_async::i2c::I2c;

/// Tiempo máximo de espera del bit de medición del BME280.
const MEASURE_TIMEOUT_MS: u32 = 20;

/// Identificador del BME280 en el registro 0xD0 (un BMP280 responde 0x58).
const BME280_CHIP_ID: u8 = 0x60;

/// Valor crudo que entrega el sensor cuando no hizo ninguna medición, por
/// ejemplo tras perder la alimentación y volver en modo de reposo.
const SKIPPED_SAMPLE: i32 = 0x80000;

/// Errores del driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bme280Error<E> {
    /// Fallo del bus I2C.
    Bus(E),
    /// En la dirección responde otro chip (por ejemplo, un BMP280 sin humedad).
    WrongChip(u8),
    /// El sensor no midió: suele indicar que se reinició y hay que configurarlo.
    Skipped,
}

pub struct Bme280<I2C> {
    i2c: I2C,
    address: u8,
    calib: CalibrationData,
}

#[derive(Default, Debug, Clone, Copy, Format)]
pub struct MeasurementsBME280 {
    pub temperature: f32,
    pub humidity: f32,
    pub pressure: f32,
}

#[derive(Default, Debug, Clone, Copy)]
struct CalibrationData {
    dig_t1: u16,
    dig_t2: i16,
    dig_t3: i16,

    dig_p1: u16,
    dig_p2: i16,
    dig_p3: i16,
    dig_p4: i16,
    dig_p5: i16,
    dig_p6: i16,
    dig_p7: i16,
    dig_p8: i16,
    dig_p9: i16,

    dig_h1: u8,
    dig_h2: i16,
    dig_h3: u8,
    dig_h4: i16,
    dig_h5: i16,
    dig_h6: i8,
}

impl<I2C> Bme280<I2C>
where
    I2C: I2c,
{
    pub async fn new(i2c: I2C, address: u8) -> Result<Self, Bme280Error<I2C::Error>> {
        let mut dev = Self {
            i2c,
            address,
            calib: CalibrationData::default(),
        };

        let mut chip_id = [0u8; 1];
        dev.read_regs(0xD0, &mut chip_id)
            .await
            .map_err(Bme280Error::Bus)?;
        if chip_id[0] != BME280_CHIP_ID {
            return Err(Bme280Error::WrongChip(chip_id[0]));
        }

        dev.init().await.map_err(Bme280Error::Bus)?;
        Ok(dev)
    }

    async fn init(&mut self) -> Result<(), I2C::Error> {
        // Reinicio del sensor
        self.write_reg(0xE0, 0xB6).await?;

        // El datasheet exige esperar 2 ms tras el reset antes de escribir
        // los registros de configuración (copia de la NVM interna).
        Timer::after(Duration::from_millis(2)).await;

        // ctrl_hum = sobremuestreo x1
        self.write_reg(0xF2, 0x01).await?;

        // ctrl_meas = temperatura x1, presión x1, modo normal
        self.write_reg(0xF4, 0x27).await?;

        // config = tiempo de espera 1000 ms
        self.write_reg(0xF5, 0xA0).await?;

        self.read_calibration().await?;

        Ok(())
    }

    async fn write_reg(&mut self, reg: u8, val: u8) -> Result<(), I2C::Error> {
        self.i2c.write(self.address, &[reg, val]).await
    }

    async fn read_regs(&mut self, reg: u8, buf: &mut [u8]) -> Result<(), I2C::Error> {
        self.i2c.write_read(self.address, &[reg], buf).await
    }

    async fn read_calibration(&mut self) -> Result<(), I2C::Error> {
        let mut buf1 = [0u8; 26];
        self.read_regs(0x88, &mut buf1).await?;

        self.calib.dig_t1 = u16::from_le_bytes([buf1[0], buf1[1]]);
        self.calib.dig_t2 = i16::from_le_bytes([buf1[2], buf1[3]]);
        self.calib.dig_t3 = i16::from_le_bytes([buf1[4], buf1[5]]);

        self.calib.dig_p1 = u16::from_le_bytes([buf1[6], buf1[7]]);
        self.calib.dig_p2 = i16::from_le_bytes([buf1[8], buf1[9]]);
        self.calib.dig_p3 = i16::from_le_bytes([buf1[10], buf1[11]]);
        self.calib.dig_p4 = i16::from_le_bytes([buf1[12], buf1[13]]);
        self.calib.dig_p5 = i16::from_le_bytes([buf1[14], buf1[15]]);
        self.calib.dig_p6 = i16::from_le_bytes([buf1[16], buf1[17]]);
        self.calib.dig_p7 = i16::from_le_bytes([buf1[18], buf1[19]]);
        self.calib.dig_p8 = i16::from_le_bytes([buf1[20], buf1[21]]);
        self.calib.dig_p9 = i16::from_le_bytes([buf1[22], buf1[23]]);

        self.calib.dig_h1 = buf1[25];

        let mut buf2 = [0u8; 7];
        self.read_regs(0xE1, &mut buf2).await?;

        self.calib.dig_h2 = i16::from_le_bytes([buf2[0], buf2[1]]);
        self.calib.dig_h3 = buf2[2];
        // Según la referencia de Bosch, los 8 bits altos de dig_H4 y dig_H5
        // llevan signo: se interpretan como i8 antes de desplazarlos.
        self.calib.dig_h4 = (((buf2[3] as i8) as i16) << 4) | ((buf2[4] & 0x0F) as i16);
        self.calib.dig_h5 = (((buf2[5] as i8) as i16) << 4) | ((buf2[4] >> 4) as i16);
        self.calib.dig_h6 = buf2[6] as i8;

        Ok(())
    }

    pub async fn measure(&mut self) -> Result<MeasurementsBME280, Bme280Error<I2C::Error>> {
        let measurement = self.read_measurement().await.map_err(Bme280Error::Bus)?;
        measurement.ok_or(Bme280Error::Skipped)
    }

    async fn read_measurement(&mut self) -> Result<Option<MeasurementsBME280>, I2C::Error> {
        // Espera acotada a que el sensor termine la conversión. El bit 3 del
        // registro de estado (0xF3) vale 1 mientras hay una medición en curso.
        let mut status = [0u8; 1];
        let mut elapsed_ms = 0u32;
        loop {
            self.read_regs(0xF3, &mut status).await?;

            if status[0] & 0x08 == 0 {
                break;
            }

            if elapsed_ms >= MEASURE_TIMEOUT_MS {
                warn!(
                    "BME280 still measuring after {} ms, reading the last sample",
                    MEASURE_TIMEOUT_MS
                );
                break;
            }

            Timer::after(Duration::from_millis(1)).await;
            elapsed_ms += 1;
        }

        let mut data = [0u8; 8];
        self.read_regs(0xF7, &mut data).await?;

        let adc_p = ((data[0] as i32) << 12) | ((data[1] as i32) << 4) | ((data[2] as i32) >> 4);
        let adc_t = ((data[3] as i32) << 12) | ((data[4] as i32) << 4) | ((data[5] as i32) >> 4);
        let adc_h = ((data[6] as i32) << 8) | data[7] as i32;
        if adc_t == SKIPPED_SAMPLE {
            return Ok(None);
        }

        let (temperature, t_fine) = self.compensate_temp(adc_t);
        let pressure = self.compensate_pressure(adc_p, t_fine);
        let humidity = self.compensate_humidity(adc_h, t_fine);

        Ok(Some(MeasurementsBME280 {
            temperature,
            pressure,
            humidity,
        }))
    }

    fn compensate_temp(&self, adc_t: i32) -> (f32, i32) {
        let var1 =
            (((adc_t >> 3) - ((self.calib.dig_t1 as i32) << 1)) * self.calib.dig_t2 as i32) >> 11;
        let var2 = (((((adc_t >> 4) - self.calib.dig_t1 as i32)
            * ((adc_t >> 4) - self.calib.dig_t1 as i32))
            >> 12)
            * self.calib.dig_t3 as i32)
            >> 14;

        let t_fine = var1 + var2;
        let t = (t_fine * 5 + 128) >> 8;

        (t as f32 / 100.0, t_fine)
    }

    fn compensate_pressure(&self, adc_p: i32, t_fine: i32) -> f32 {
        let mut var1 = t_fine as i64 - 128000;
        let mut var2 = var1 * var1 * self.calib.dig_p6 as i64;
        var2 += (var1 * self.calib.dig_p5 as i64) << 17;
        var2 += (self.calib.dig_p4 as i64) << 35;
        var1 = ((var1 * var1 * self.calib.dig_p3 as i64) >> 8)
            + ((var1 * self.calib.dig_p2 as i64) << 12);
        var1 = (((1i64 << 47) + var1) * self.calib.dig_p1 as i64) >> 33;

        if var1 == 0 {
            return 0.0;
        }

        let mut p = 1048576 - adc_p as i64;
        p = (((p << 31) - var2) * 3125) / var1;
        var1 = (self.calib.dig_p9 as i64 * (p >> 13) * (p >> 13)) >> 25;
        var2 = (self.calib.dig_p8 as i64 * p) >> 19;

        p = ((p + var1 + var2) >> 8) + ((self.calib.dig_p7 as i64) << 4);

        // `p` viene en Pa en formato Q24.8 (/ 256); se divide entre 100 adicional para entregar hPa (300..=1100).
        (p as f32) / 25600.0
    }

    fn compensate_humidity(&self, adc_h: i32, t_fine: i32) -> f32 {
        let mut v = t_fine - 76800;

        v = ((((adc_h << 14)
            - ((self.calib.dig_h4 as i32) << 20)
            - ((self.calib.dig_h5 as i32) * v))
            + 16384)
            >> 15)
            * (((((((v * self.calib.dig_h6 as i32) >> 10)
                * (((v * self.calib.dig_h3 as i32) >> 11) + 32768))
                >> 10)
                + 2097152)
                * self.calib.dig_h2 as i32
                + 8192)
                >> 14);

        v -= ((((v >> 15) * (v >> 15)) >> 7) * self.calib.dig_h1 as i32) >> 4;

        v = v.clamp(0, 419430400);

        (v >> 12) as f32 / 1024.0
    }
}
