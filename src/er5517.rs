use embedded_hal::{
    blocking::{
        delay::DelayMs,
        spi::{Transfer as SPITransfer, Write as SPIWrite},
    },
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
    SPI: SPIWrite<u8, Error = SPIErr> + SPITransfer<u8, Error = SPIErr>,
    CS: OutputPin<Error = PinErr>,
{
    fn with_select<T, F: FnOnce(&mut SPI) -> T>(&mut self, f: F) -> Res<T, PinErr, SPIErr> {
        self.cs.set_low().map_err(Error::Pin)?;
        let r = f(&mut self.spi);
        self.cs.set_high().map_err(Error::Pin)?;
        Ok(r)
    }

    fn write(&mut self, bytes: &[u8]) -> Res<(), PinErr, SPIErr> {
        let r = self.with_select(|spi| spi.write(bytes))?;
        r.map_err(Error::SPI)
    }

    fn read<'w>(&mut self, bytes: &'w mut [u8]) -> Res<&'w [u8], PinErr, SPIErr> {
        let r = self.with_select(|spi| spi.transfer(bytes))?;
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

    pub fn read_status(&mut self) -> Res<u8, PinErr, SPIErr> {
        let mut d: [u8; 2] = [0x40, 0x00];
        let v = self.read(&mut d)?;
        Ok(v[1])
    }

    pub fn read_data(&mut self) -> Res<u8, PinErr, SPIErr> {
        let mut d: [u8; 2] = [0xc0, 0x00];
        let v = self.read(&mut d)?;
        Ok(v[1])
    }

    pub fn color_bars(&mut self, on: bool) -> Res<(), PinErr, SPIErr> {
        self.write_cmd(0x12)?;
        let mask = 0b0010_0000;

        let mut s = self.read_data()?;
        s = if on { s | mask } else { s & !mask };

        self.write_data(s)
    }

    pub fn on(&mut self, on: bool) -> Res<(), PinErr, SPIErr> {
        self.write_cmd(0x12)?;
        let mask = 0b0100_0000u8;

        let mut s = self.read_data()?;
        s = if on { s | mask } else { s & !mask };
        self.write_data(s)
    }
}
