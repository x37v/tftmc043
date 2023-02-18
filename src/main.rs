#![no_std]
#![no_main]

mod er5517;

use panic_halt as _;

#[rtic::app(device = rp_pico::hal::pac, peripherals = true)]
mod app {

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

        let _ = enable.set_high();

        let _ = spi_cs.set_high();

        // Exchange the uninitialised SPI driver for an initialised one
        let spi = spi.init(
            &mut resets,
            clocks.peripheral_clock.freq(),
            16.MHz(),
            &embedded_hal::spi::MODE_0,
        );

        let mut display = crate::er5517::ER5517::new(spi, spi_cs);

        let _ = display.color_bars();

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
