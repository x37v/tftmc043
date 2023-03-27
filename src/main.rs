#![no_std]
#![no_main]

use panic_halt as _;

#[rtic::app(device = rp_pico::hal::pac, peripherals = true)]
mod app {
    use tftmc043::{ColorMode, TFTMC043};

    use embedded_graphics::{
        mock_display::MockDisplay,
        mono_font::{ascii::FONT_6X10, MonoTextStyle},
        pixelcolor::Rgb888,
        prelude::*,
        primitives::{
            Circle, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, StrokeAlignment, Triangle,
        },
        text::{Alignment, Text},
    };
    use embedded_hal::{digital::v2::OutputPin, prelude::*};
    use fugit::{MicrosDurationU32, RateExtU32};
    use rp_pico::{
        hal::{
            self, clocks::init_clocks_and_plls, prelude::*, timer::Alarm, watchdog::Watchdog, Sio,
        },
        XOSC_CRYSTAL_FREQ,
    };

    const SCAN_TIME_US: MicrosDurationU32 = MicrosDurationU32::secs(1);

    #[shared]
    struct Shared {
        timer: hal::Timer,
        alarm: hal::timer::Alarm0,
        led: hal::gpio::Pin<hal::gpio::pin::bank0::Gpio25, hal::gpio::PushPullOutput>,
    }

    #[local]
    struct Local {}

    #[init]
    fn init(c: init::Context) -> (Shared, Local, init::Monotonics) {
        // Soft-reset does not release the hardware spinlocks
        // Release them now to avoid a deadlock after debug or watchdog reset
        unsafe {
            hal::sio::spinlock_reset();
        }
        let mut resets = c.device.RESETS;
        let mut watchdog = Watchdog::new(c.device.WATCHDOG);
        let clocks = init_clocks_and_plls(
            XOSC_CRYSTAL_FREQ,
            c.device.XOSC,
            c.device.CLOCKS,
            c.device.PLL_SYS,
            c.device.PLL_USB,
            &mut resets,
            &mut watchdog,
        )
        .ok()
        .unwrap();

        let mut delay =
            cortex_m::delay::Delay::new(c.core.SYST, clocks.system_clock.freq().to_Hz());

        let sio = Sio::new(c.device.SIO);
        let pins = rp_pico::Pins::new(
            c.device.IO_BANK0,
            c.device.PADS_BANK0,
            sio.gpio_bank0,
            &mut resets,
        );
        let mut led = pins.led.into_push_pull_output();
        led.set_low().unwrap();

        let mut timer = hal::Timer::new(c.device.TIMER, &mut resets);
        let mut alarm = timer.alarm_0().unwrap();
        let _ = alarm.schedule(SCAN_TIME_US);
        alarm.enable_interrupt();

        let _spi_sclk = pins.gpio6.into_mode::<hal::gpio::FunctionSpi>();
        let _spi_mosi = pins.gpio7.into_mode::<hal::gpio::FunctionSpi>();
        let _spi_miso = pins.gpio4.into_mode::<hal::gpio::FunctionSpi>();
        let mut spi_cs = pins.gpio5.into_push_pull_output();
        let spi = hal::Spi::<_, _, 8>::new(c.device.SPI0);

        let mut enable = pins.gpio8.into_push_pull_output();

        let _ = spi_cs.set_high();

        // Exchange the uninitialised SPI driver for an initialised one
        let spi = spi.init(
            &mut resets,
            clocks.peripheral_clock.freq(),
            8.MHz(),
            &embedded_hal::spi::MODE_0,
        );

        let mut display = TFTMC043::new(spi, spi_cs);

        let _ = enable.set_low();
        delay.delay_ms(500);
        let _ = enable.set_high();
        delay.delay_ms(500);

        let _ = display.init(&mut delay);

        let mode = ColorMode::TwentyFourBit;

        /*
        let _ = display.fg_color(mode, 0x0, 0x0, 0x0);
        let _ = display.line_start(0, 0);
        let _ = display.line_end(800, 480);
        let _ = display.rect_fill();

        let _ = display.bg_color(mode, 0x0, 0x0, 0x0);
        let _ = display.set_brightness(1);

        let _ = display.fg_color(mode, 0xff, 0x00, 0x0);
        let _ = display.line_start(10, 10);
        let _ = display.line_end(80, 80);
        let _ = display.rect_fill();
        */

        let _ = display.clear(Rgb888::new(0, 0, 0));

        let fill = PrimitiveStyle::with_fill(Rgb888::new(0, 255, 0));
        let character_style = MonoTextStyle::new(&FONT_6X10, Rgb888::new(255, 255, 0));

        let _ = Rectangle::new(Point::new(52, 10), Size::new(16, 16))
            .into_styled(fill)
            .draw(&mut display);

        let text = "embedded-graphics";
        Text::with_alignment(
            text,
            display.bounding_box().center() + Point::new(0, 15),
            character_style,
            Alignment::Center,
        )
        .draw(&mut display)
        .ok();

        /* The ER-TFTMC043-3 provides a color bar display, which can be used as a display test and does not require display
        memory. The function can be performed by Host to set REG[12h] bit5 to 1 */

        (Shared { timer, alarm, led }, Local {}, init::Monotonics())
    }

    #[task(
        binds = TIMER_IRQ_0,
        priority = 1,
        shared = [timer, alarm, led],
        local = [tog: bool = true],
    )]
    fn timer_irq(mut c: timer_irq::Context) {
        if *c.local.tog {
            c.shared.led.lock(|l| l.set_high().unwrap());
        } else {
            c.shared.led.lock(|l| l.set_low().unwrap());
        }
        *c.local.tog = !*c.local.tog;

        let mut alarm = c.shared.alarm;
        (alarm).lock(|a| {
            a.clear_interrupt();
            let _ = a.schedule(SCAN_TIME_US);
        });
    }
}
