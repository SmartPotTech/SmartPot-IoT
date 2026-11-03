pub mod bme280;
pub mod tca9548a;
//pub mod tds;

pub trait Analog {
    fn ready(&mut self);
}
