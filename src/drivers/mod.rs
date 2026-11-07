pub mod bh1750;
pub mod bme280;
pub mod dht22;
pub mod dht22_frame;
pub mod hd44780;
pub mod tca9548a;
pub mod tds;

pub trait Analog {
    fn ready(&mut self);
}
