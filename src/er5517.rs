use embedded_hal::{
    blocking::{
        delay::DelayMs,
        spi::{Transfer as SPITransfer, Write as SPIWrite},
    },
    digital::v2::OutputPin,
};
///LCD size: width, height
pub const LCD_SIZE: (usize, usize) = (800, 480);

const LCD_HBPD: u16 = 140;
const LCD_HFPD: u16 = 160;
const LCD_HSPW: u16 = 20;

const LCD_VBPD: u16 = 20;
const LCD_VFPD: u16 = 12;
const LCD_VSPW: u16 = 3;

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum ColorMode {
    EightBit,
    SixteenBit,
    TwentyFourBit,
}

fn color_mode(mode: ColorMode, mut r: u8, mut g: u8, mut b: u8) -> (u8, u8, u8) {
    match mode {
        ColorMode::EightBit => {
            r = r.clamp(0, 0b0000_0111) << 5;
            g = g.clamp(0, 0b0000_0111) << 5;
            b = b.clamp(0, 0b0000_0011) << 6;
        }
        ColorMode::SixteenBit => {
            r = r.clamp(0, 0b0001_1111) << 3;
            g = g.clamp(0, 0b0011_1111) << 2;
            b = b.clamp(0, 0b0001_1111) << 3;
        }
        _ => (),
    };
    (r, g, b)
}

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

    pub fn select_main_window_color_mode(&mut self, mode: ColorMode) -> Res<(), PinErr, SPIErr> {
        self.cmd_write(0x10)?;
        let v = (self.data_read()? & !0b1100)
            | match mode {
                ColorMode::EightBit => 0b0000,
                ColorMode::SixteenBit => 0b0100,
                ColorMode::TwentyFourBit => 0b1000,
            };
        self.data_write(v)
    }

    pub fn init(&mut self, delay: &mut dyn DelayMs<u16>) -> Res<(), PinErr, SPIErr> {
        self.system_check_temp(delay)?;
        delay.delay_ms(100);
        while self.status_read()? & 0x02 != 0 {
            //loop
        }

        self.pll_init(delay)?;
        self.sdram_init(delay)?;

        self.tft_16bit()?;
        self.host_16bit()?;

        self.rgb_16bit_16bpp()?;
        self.memwrite_left_right_top_down()?;

        self.graphic_mode()?;
        self.mem_select_sdram()?;

        self.hscan_l_to_r()?; //REG[12h]:from left to right
        self.vscan_t_to_b()?; //REG[12h]:from top to bottom
        self.pdata_set_rgb()?; //REG[12h]:Select RGB output

        self.pclk_falling()?;
        self.hsync_low_active()?;
        self.vsync_low_active()?;
        self.de_high_active()?;

        self.set_width_height(LCD_SIZE.0 as _, LCD_SIZE.1 as _)?;
        self.set_horiz_non_display(LCD_HBPD)?;
        self.set_horiz_start_pos(LCD_HFPD)?;
        self.set_horiz_pulse_width(LCD_HSPW)?;
        self.set_vert_non_display(LCD_VBPD)?;
        self.set_vert_start_pos(LCD_VFPD)?;
        self.set_vert_pulse_width(LCD_VSPW)?;

        self.select_main_window_color_mode(ColorMode::TwentyFourBit)?;
        self.memory_xy_mode()?;
        self.memory_color_mode(ColorMode::TwentyFourBit)?;
        self.select_main_window_color_mode(ColorMode::TwentyFourBit)?;

        self.on(true)?;

        self.select_main_window_color_mode(ColorMode::TwentyFourBit)?;
        self.main_image(0, 0, 0, LCD_SIZE.0 as _)?;
        self.canvas_image(0, LCD_SIZE.0 as _)?;
        self.active_window(0, 0, LCD_SIZE.0 as _, LCD_SIZE.1 as _)?;
        Ok(())
    }

    pub fn fg_color(&mut self, mode: ColorMode, r: u8, g: u8, b: u8) -> Res<(), PinErr, SPIErr> {
        let (r, g, b) = color_mode(mode, r, g, b);

        self.register_write(0xD2, r)?;
        self.register_write(0xD3, g)?;
        self.register_write(0xD4, b)?;

        Ok(())
    }

    pub fn bg_color(&mut self, mode: ColorMode, r: u8, g: u8, b: u8) -> Res<(), PinErr, SPIErr> {
        let (r, g, b) = color_mode(mode, r, g, b);
        self.register_write(0xD5, r)?;
        self.register_write(0xD6, g)?;
        self.register_write(0xD7, b)?;
        Ok(())
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
        while self.status_read()? & 0x08 != 0 {
            //busy loop
        }
        Ok(())
    }

    fn system_check_temp(&mut self, delay: &mut dyn DelayMs<u16>) -> Res<(), PinErr, SPIErr> {
        loop {
            if self.status_read()? & 0x02 == 0 {
                delay.delay_ms(2);
                self.cmd_write(0x01)?;
                delay.delay_ms(2);
                if self.data_read()? & 0x80 == 0x80 {
                    return Ok(());
                }
                delay.delay_ms(2);
                self.cmd_write(0x01)?;
                delay.delay_ms(2);
                self.data_write(0x80)?;
            }
        }
    }

    fn pll_init(&mut self, delay: &mut dyn DelayMs<u16>) -> Res<(), PinErr, SPIErr> {
        let lpllOD_sclk = 2u8;
        let lpllOD_cclk = 2u8;
        let lpllOD_mclk = 2u8;
        let lpllR_sclk = 5u8;
        let lpllR_cclk = 5u8;
        let lpllR_mclk = 5u8;
        let lpllN_sclk = 50u8; // TFT PCLK out put frequency:65
        let lpllN_cclk = 100u8; // Core CLK:100
        let lpllN_mclk = 100u8; // SRAM CLK:100
                                //
        self.register_write(0x05, (lpllOD_sclk << 6) | (lpllR_sclk << 1))?;
        self.register_write(0x07, (lpllOD_mclk << 6) | (lpllR_mclk << 1))?;
        self.register_write(0x09, (lpllOD_cclk << 6) | (lpllR_cclk << 1))?;

        self.register_write(0x06, lpllN_sclk)?;
        self.register_write(0x08, lpllN_mclk)?;
        self.register_write(0x0a, lpllN_cclk)?;

        self.cmd_write(0x00)?;
        delay.delay_ms(1);
        self.data_write(0x80)?;
        delay.delay_ms(1);

        //set pwm0 pwm1 100%
        self.register_write(0x85, 0x0a)?;
        self.register_write(0x88, 0x64)?;
        self.register_write(0x8a, 0x64)?;
        self.register_write(0x8c, 0x64)?;
        self.register_write(0x8e, 0x64)?;
        self.register_write(0x86, 0x33)
    }

    fn sdram_init(&mut self, delay: &mut dyn DelayMs<u16>) -> Res<(), PinErr, SPIErr> {
        self.register_write(0xe0, 0x29)?;
        self.register_write(0xe1, 0x03)?; //CAS:2=0x02�ACAS:3=0x03

        let sdram_itv = 476u16; //(64000000 / 8192) / (1000/60) - 12
        self.register_write(0xe2, sdram_itv as u8)?;
        self.register_write(0xe3, (sdram_itv >> 8) as u8)?;
        self.register_write(0xe4, 0x01)?;
        self.sdram_check_ready()?;
        delay.delay_ms(1);
        Ok(())
    }

    fn sdram_check_ready(&mut self) -> Res<(), PinErr, SPIErr> {
        while self.status_read()? & 0x04 == 0 {
            //LOOP
        }
        Ok(())
    }

    fn tft_16bit(&mut self) -> Res<(), PinErr, SPIErr> {
        self.cmd_write(0x01)?;
        let v = (self.data_read()? | 0b1_0000) & !0b1000;
        self.data_write(v)
    }

    fn host_16bit(&mut self) -> Res<(), PinErr, SPIErr> {
        self.cmd_write(0x01)?;
        let v = self.data_read()? | 0b0001;
        self.data_write(v)
    }

    fn rgb_16bit_16bpp(&mut self) -> Res<(), PinErr, SPIErr> {
        self.cmd_write(0x02)?;
        let v = (self.data_read()? | 0b0100_0000) & !0b1000_0000;
        self.data_write(v)
    }

    fn memwrite_left_right_top_down(&mut self) -> Res<(), PinErr, SPIErr> {
        self.cmd_write(0x02)?;
        let v = self.data_read()? & !0b0000_0110;
        self.data_write(v)
    }

    fn graphic_mode(&mut self) -> Res<(), PinErr, SPIErr> {
        self.cmd_write(0x03)?;
        let v = self.data_read()? & !0b0000_0100;
        self.data_write(v)
    }

    fn mem_select_sdram(&mut self) -> Res<(), PinErr, SPIErr> {
        self.cmd_write(0x03)?;
        let v = self.data_read()? & !0b0000_0011;
        self.data_write(v)
    }

    fn hscan_l_to_r(&mut self) -> Res<(), PinErr, SPIErr> {
        self.cmd_write(0x12)?;
        let v = self.data_read()? & !0b0001_0000;
        self.data_write(v)
    }

    fn vscan_t_to_b(&mut self) -> Res<(), PinErr, SPIErr> {
        self.cmd_write(0x12)?;
        let v = self.data_read()? & !0b0000_1000;
        self.data_write(v)
    }

    fn pdata_set_rgb(&mut self) -> Res<(), PinErr, SPIErr> {
        self.cmd_write(0x12)?;
        let v = self.data_read()? & !0b0000_0111;
        self.data_write(v)
    }

    fn set_width_height(&mut self, w: u16, h: u16) -> Res<(), PinErr, SPIErr> {
        self.register_write(0x14, (w / 8 - 1) as _)?;
        self.register_write(0x15, (w % 8) as _)?;
        self.register_write(0x1A, (h - 1) as _)?;
        self.register_write(0x1B, ((h - 1) >> 8) as _)?;
        Ok(())
    }

    fn set_horiz_non_display(&mut self, w: u16) -> Res<(), PinErr, SPIErr> {
        self.register_write(0x16, (w / 8 - 1) as _)?;
        self.register_write(0x17, (w % 8) as _)?;
        Ok(())
    }

    fn set_horiz_start_pos(&mut self, w: u16) -> Res<(), PinErr, SPIErr> {
        self.register_write(0x18, (w / 8).saturating_sub(1) as _)?;
        Ok(())
    }

    fn set_horiz_pulse_width(&mut self, w: u16) -> Res<(), PinErr, SPIErr> {
        self.register_write(0x19, (w / 8).saturating_sub(1) as _)?;
        Ok(())
    }

    fn set_vert_non_display(&mut self, v: u16) -> Res<(), PinErr, SPIErr> {
        let v = v - 1;
        self.register_write(0x1c, v as _)?;
        self.register_write(0x1d, (v >> 8) as _)?;
        Ok(())
    }

    fn set_vert_start_pos(&mut self, v: u16) -> Res<(), PinErr, SPIErr> {
        self.register_write(0x1e, v.saturating_sub(1) as _)?;
        Ok(())
    }

    fn set_vert_pulse_width(&mut self, v: u16) -> Res<(), PinErr, SPIErr> {
        self.register_write(0x1f, v.saturating_sub(1) as _)?;
        Ok(())
    }

    fn memory_xy_mode(&mut self) -> Res<(), PinErr, SPIErr> {
        self.cmd_write(0x5e)?;
        let v = self.data_read()? & !0b0000_0100;
        self.data_write(v)?;
        Ok(())
    }

    pub fn memory_color_mode(&mut self, mode: ColorMode) -> Res<(), PinErr, SPIErr> {
        self.cmd_write(0x5e)?;
        let v = (self.data_read()? & !0b0011)
            | match mode {
                ColorMode::EightBit => 0b00,
                ColorMode::SixteenBit => 0b01,
                ColorMode::TwentyFourBit => 0b10,
            };

        self.data_write(v)?;
        Ok(())
    }

    fn pclk_falling(&mut self) -> Res<(), PinErr, SPIErr> {
        self.cmd_write(0x12)?;
        let v = self.data_read()? | 0b1000_0000;
        self.data_write(v)?;
        Ok(())
    }

    fn hsync_low_active(&mut self) -> Res<(), PinErr, SPIErr> {
        self.cmd_write(0x13)?;
        let v = self.data_read()? & !0b1000_0000;
        self.data_write(v)?;
        Ok(())
    }

    fn vsync_low_active(&mut self) -> Res<(), PinErr, SPIErr> {
        self.cmd_write(0x13)?;
        let v = self.data_read()? & !0b0100_0000;
        self.data_write(v)?;
        Ok(())
    }

    fn de_high_active(&mut self) -> Res<(), PinErr, SPIErr> {
        self.cmd_write(0x13)?;
        let v = self.data_read()? & !0b0010_0000;
        self.data_write(v)?;
        Ok(())
    }

    fn set_pwm_prescaler_1_to_256(&mut self, v: u16) -> Res<(), PinErr, SPIErr> {
        self.register_write(0x84, v.saturating_sub(1) as _)?;
        Ok(())
    }

    fn select_pwm1_clock_div_by_1(&mut self) -> Res<(), PinErr, SPIErr> {
        /*
        Select MUX input for PWM Timer 1.
        00 = 1; 01 = 1/2; 10 = 1/4 ; 11 = 1/8;
        */
        self.cmd_write(0x85)?;
        let v = self.data_read()? & !0b1100_0000;
        self.data_write(v)?;
        Ok(())
    }

    fn select_pwm1(&mut self) -> Res<(), PinErr, SPIErr> {
        self.cmd_write(0x85)?;
        let v = (self.data_read()? | 0b1000) & !0b0100;
        self.data_write(v)?;
        Ok(())
    }

    fn start_pwm1(&mut self) -> Res<(), PinErr, SPIErr> {
        self.cmd_write(0x86)?;
        let v = self.data_read()? | 0b1_0000;
        self.data_write(v)?;
        Ok(())
    }

    fn set_timer1_count_buffer(&mut self, v: u16) -> Res<(), PinErr, SPIErr> {
        self.register_write(0x8e, v as _)?;
        self.register_write(0x8f, (v >> 8) as _)?;
        Ok(())
    }

    fn set_timer1_compare_buffer(&mut self, v: u16) -> Res<(), PinErr, SPIErr> {
        self.register_write(0x8c, v as _)?;
        self.register_write(0x8d, (v >> 8) as _)?;
        Ok(())
    }

    pub fn set_brightness(&mut self, v: u16) -> Res<(), PinErr, SPIErr> {
        self.select_pwm1()?;
        self.set_pwm_prescaler_1_to_256(20)?;
        self.select_pwm1_clock_div_by_1()?;
        self.set_timer1_count_buffer(100)?;
        self.set_timer1_compare_buffer(v)?;
        self.start_pwm1()?;
        Ok(())
    }
}
