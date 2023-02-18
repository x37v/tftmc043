use embedded_hal::{
    blocking::{
        delay::DelayMs,
        spi::{Transfer as SPITransfer, Write as SPIWrite},
    },
    digital::v2::OutputPin,
};
///LCD size: width, height
pub const LCD_SIZE: (usize, usize) = (800, 480);

pub enum Error<P = (), S = ()> {
    Pin(P),
    SPI(S),
}

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

    fn cmd_write(&mut self, cmd: u8) -> Res<(), PinErr, SPIErr> {
        self.write(&[0, cmd])
    }

    fn data_write(&mut self, data: u8) -> Res<(), PinErr, SPIErr> {
        self.write(&[0x80, data])
    }

    pub fn new(spi: SPI, cs: CS) -> Self {
        Self { spi, cs }
    }

    pub fn status_read(&mut self) -> Res<u8, PinErr, SPIErr> {
        let mut d: [u8; 2] = [0x40, 0x00];
        let v = self.read(&mut d)?;
        Ok(v[1])
    }

    pub fn data_read(&mut self) -> Res<u8, PinErr, SPIErr> {
        let mut d: [u8; 2] = [0xc0, 0x00];
        let v = self.read(&mut d)?;
        Ok(v[1])
    }

    pub fn register_write(&mut self, cmd: u8, data: u8) -> Res<(), PinErr, SPIErr> {
        self.cmd_write(cmd)?;
        self.data_write(data)
    }

    pub fn color_bars(&mut self, on: bool) -> Res<(), PinErr, SPIErr> {
        self.cmd_write(0x12)?;
        let mask = 0b0010_0000;

        let mut s = self.data_read()?;
        s = if on { s | mask } else { s & !mask };

        self.data_write(s)
    }

    pub fn on(&mut self, on: bool) -> Res<(), PinErr, SPIErr> {
        self.cmd_write(0x12)?;
        let mask = 0b0100_0000u8;

        let mut s = self.data_read()?;
        s = if on { s | mask } else { s & !mask };
        self.data_write(s)
    }

    pub fn select_main_window_16bpp(&mut self) -> Res<(), PinErr, SPIErr> {
        self.cmd_write(0x10)?;
        let mut v = self.data_read()?;
        v &= !0b1000;
        v |= 0b0100;
        self.data_write(v)
    }

    pub fn init(&mut self) -> Res<(), PinErr, SPIErr> {
        self.on(true)?;
        self.select_main_window_16bpp()?;
        self.main_image(0, 0, 0, LCD_SIZE.0 as _)?;
        self.canvas_image(0, LCD_SIZE.0 as _)?;
        self.active_window(0, 0, LCD_SIZE.0 as _, LCD_SIZE.1 as _)
    }

    pub fn fg_color_65k(&mut self, r: u8, g: u8, b: u8) -> Res<(), PinErr, SPIErr> {
        self.register_write(0xD2, r.clamp(0, 0b1_1111))?;
        self.register_write(0xD3, g.clamp(0, 0b11_1111))?;
        self.register_write(0xD4, b.clamp(0, 0b11_1111))
    }

    pub fn bg_color_65k(&mut self, r: u8, g: u8, b: u8) -> Res<(), PinErr, SPIErr> {
        self.register_write(0xD5, r.clamp(0, 0b1_1111))?;
        self.register_write(0xD6, g.clamp(0, 0b11_1111))?;
        self.register_write(0xD7, b.clamp(0, 0b11_1111))
    }

    pub fn active_window(&mut self, x: u16, y: u16, w: u16, h: u16) -> Res<(), PinErr, SPIErr> {
        self.register_write(0x56, x as u8)?;
        self.register_write(0x57, (x >> 8) as u8)?;
        self.register_write(0x58, y as u8)?;
        self.register_write(0x59, (y >> 8) as u8)?;

        self.register_write(0x5a, w as u8)?;
        self.register_write(0x5b, (w >> 8) as u8)?;
        self.register_write(0x5c, h as u8)?;
        self.register_write(0x5d, (h >> 8) as u8)
    }

    pub fn line_start(&mut self, x: u16, y: u16) -> Res<(), PinErr, SPIErr> {
        self.register_write(0x68, x as u8)?;
        self.register_write(0x69, (x >> 8) as u8)?;
        self.register_write(0x6a, y as u8)?;
        self.register_write(0x6b, (y >> 8) as u8)
    }

    pub fn line_end(&mut self, x: u16, y: u16) -> Res<(), PinErr, SPIErr> {
        self.register_write(0x6c, x as u8)?;
        self.register_write(0x6d, (x >> 8) as u8)?;
        self.register_write(0x6e, y as u8)?;
        self.register_write(0x6f, (y >> 8) as u8)
    }

    pub fn rect_fill(&mut self) -> Res<(), PinErr, SPIErr> {
        self.register_write(0x76, 0xE0)?;
        self.busy_draw()
    }

    pub fn main_image(&mut self, addr: u32, x: u16, y: u16, w: u16) -> Res<(), PinErr, SPIErr> {
        self.register_write(0x20, addr as _)?;
        self.register_write(0x21, (addr >> 8) as _)?;
        self.register_write(0x22, (addr >> 16) as _)?;
        self.register_write(0x23, (addr >> 24) as _)?;

        self.register_write(0x24, w as _)?;
        self.register_write(0x25, (w >> 8) as _)?;

        self.register_write(0x26, x as _)?;
        self.register_write(0x27, (x >> 8) as _)?;

        self.register_write(0x28, y as _)?;
        self.register_write(0x29, (y >> 8) as _)
    }

    pub fn canvas_image(&mut self, addr: u32, w: u16) -> Res<(), PinErr, SPIErr> {
        self.register_write(0x50, addr as _)?;
        self.register_write(0x51, (addr >> 8) as _)?;
        self.register_write(0x52, (addr >> 16) as _)?;
        self.register_write(0x53, (addr >> 24) as _)?;

        self.register_write(0x54, w as _)?;
        self.register_write(0x55, (w >> 8) as _)
    }

    pub fn busy_draw(&mut self) -> Res<(), PinErr, SPIErr> {
        loop {
            let v = self.status_read()?;
            if v & 0x08 == 0 {
                return Ok(());
            }
        }
    }
}
