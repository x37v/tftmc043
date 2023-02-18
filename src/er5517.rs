use embedded_hal::{
    blocking::{delay::DelayMs, spi::Write as SPIWrite},
    digital::v2::OutputPin,
};

pub enum Error<P = (), S = ()> {
    Pin(P),
    SPI(S),
}

///LCD size: width, height
pub const LCD_SIZE: (usize, usize) = (800, 480);

pub struct ER5517<SPI, CS> {
    spi: SPI,
    cs: CS, //chip select
}

type Res<T, P, S> = Result<T, Error<P, S>>;

impl<SPI, CS, PinErr, SPIErr> ER5517<SPI, CS>
where
    SPI: SPIWrite<u8, Error = SPIErr>,
    CS: OutputPin<Error = PinErr>,
{
    fn with_select<T, F: Fn(&mut SPI) -> T>(&mut self, f: F) -> Res<T, PinErr, SPIErr> {
        self.cs.set_low().map_err(Error::Pin)?;
        let r = f(&mut self.spi);
        self.cs.set_high().map_err(Error::Pin)?;
        Ok(r)
    }

    fn write(&mut self, bytes: &[u8]) -> Res<(), PinErr, SPIErr> {
        let r = self.with_select(|spi| spi.write(bytes))?;
        r.map_err(Error::SPI)
    }

    fn write_cmd(&mut self, cmd: u8) -> Res<(), PinErr, SPIErr> {
        self.write(&[0, cmd])
    }

    fn write_data(&mut self, data: u8) -> Res<(), PinErr, SPIErr> {
        self.write(&[0x80, data])
    }

    pub fn new(spi: SPI, cs: CS) -> Self {
        Self { spi, cs }
    }

    pub fn color_bars(&mut self) -> Res<(), PinErr, SPIErr> {
        self.write_cmd(0x12)?;
        self.write_data(0b0110_0000)
    }
}
