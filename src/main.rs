#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

use panic_rtt_target as _;

mod config;
mod errors;
mod flash;
mod nvm;
mod voltage;

use stm32l0xx_hal as hal;

#[rtic::app(device = stm32l0xx_hal::pac, dispatchers = [SPI2])]
mod app {

    use crate::flash::SpiFlash;
    use crate::config;
    use crate::hal::{
        delay::Delay,
        gpio::{
            gpioa::PA8,
            gpiob::{PB0, PB1, PB2, PB3, PB4, PB5, PB7},
            Output, PushPull,
        },
        prelude::*,
        rcc::Config,
        signature::device_id,
        spi::{Spi, MODE_0},
        syscfg::SYSCFG,
        usb::{UsbBus, USB},
    };
    use epd_waveshare::{epd1in54_v2::*, prelude::*};
    use hex_display::HexDisplayExt;
    use rtic_sync::channel::Receiver;
    use rtic_sync::{channel::*, make_channel};
    use rtt_target::{rprintln, rtt_init_print};
    use shared_bus::{BusManager, NullMutex, SpiProxy};
    use usb_device::{
        bus::UsbBusAllocator,
        prelude::{UsbDevice, UsbDeviceBuilder, UsbVidPid},
    };
    use usbd_webusb::{url_scheme, WebUsb};

    type BusMgrInner = NullMutex<
        Spi<
            stm32l0xx_hal::pac::SPI1,
            (
                PB3<stm32l0xx_hal::gpio::Analog>,
                PB4<stm32l0xx_hal::gpio::Analog>,
                PB5<stm32l0xx_hal::gpio::Analog>,
            ),
        >,
    >;
    type BusMgr = BusManager<BusMgrInner>;

    #[shared]
    struct Shared {
        flash: SpiFlash<'static>
    }

    #[local]
    struct Local {
        delay: Delay,
        epd: Epd1in54<
            SpiProxy<'static, BusMgrInner>,
            PB2<stm32l0xx_hal::gpio::Output<stm32l0xx_hal::gpio::PushPull>>,
            PB7<stm32l0xx_hal::gpio::Input<stm32l0xx_hal::gpio::Floating>>,
            PB1<stm32l0xx_hal::gpio::Output<stm32l0xx_hal::gpio::PushPull>>,
            PB0<stm32l0xx_hal::gpio::Output<stm32l0xx_hal::gpio::PushPull>>,
            stm32l0xx_hal::delay::Delay,
        >,
        led_b: PA8<Output<PushPull>>,
        spi_epd: SpiProxy<'static, BusMgrInner>,
        usb_dev: UsbDevice<'static, UsbBus<USB>>,
        webusb: WebUsb<UsbBus<USB>>,
    }

    const MSG_Q_CAPACITY: usize = 1;
    #[init(local = [USB_BUS: Option<UsbBusAllocator<UsbBus<USB>>> = None, SPI_BUS: Option<BusMgr> = None])]
    fn init(cx: init::Context) -> (Shared, Local) {
        rtt_init_print!();
        rprintln!("Hello, world!");

        let p = cx.device;
        let cp = cx.core;
        let mut rcc = p.RCC.freeze(Config::hsi16());
        let mut syscfg = SYSCFG::new(p.SYSCFG, &mut rcc);
        let hsi48 = rcc.enable_hsi48(&mut syscfg, p.CRS);

        // gpioa
        let gpioa = p.GPIOA.split(&mut rcc);

        // gpiob
        let gpiob = p.GPIOB.split(&mut rcc);
        let rst = gpiob.pb0.into_push_pull_output();
        let dc = gpiob.pb1.into_push_pull_output();
        let cs_epd = gpiob.pb2.into_push_pull_output();
        let sck = gpiob.pb3;
        let miso = gpiob.pb4;
        let mosi = gpiob.pb5;
        let cs_flash = gpiob.pb6.into_push_pull_output();
        let busy_in = gpiob.pb7.into_floating_input();

        let usb = USB::new(p.USB, gpioa.pa11, gpioa.pa12, hsi48);
        let mut delay = Delay::new(cp.SYST, rcc.clocks);

        // trick to make usb_bus live forever, lifted from
        // https://github.com/rtic-rs/rtic-examples/blob/master/rtic_v1/stm32f0_hid_mouse/src/main.rs
        let usb_bus = cx.local.USB_BUS;
        *usb_bus = Some(UsbBus::new(usb));

        let spi = p
            .SPI1
            .spi((sck, miso, mosi), MODE_0, 4_000_000.Hz(), &mut rcc);

        // Create a shared SPI bus and also make it live forever
        let spi_bus = cx.local.SPI_BUS;
        *spi_bus = Some(shared_bus::BusManagerSimple::new(spi));
        let mut spi_epd = spi_bus.as_ref().unwrap().acquire_spi();
        let spi_flash = spi_bus.as_ref().unwrap().acquire_spi();

        // Setup EPD
        rprintln!("Setup EPD...");
        let mut epd =
            Epd1in54::new(&mut spi_epd, cs_epd, busy_in, dc, rst, &mut delay, None).unwrap();
        let mut flash = SpiFlash::new(spi_flash, cs_flash, &mut delay);
        let mut config = config::FlashConfig::from_flash(&mut flash);

        let webusb = WebUsb::new(
            usb_bus.as_ref().unwrap(),
            url_scheme::HTTPS,
            "cardonabits.github.io/lightnote-app/",
        );
        let usb_dev = UsbDeviceBuilder::new(usb_bus.as_ref().unwrap(), UsbVidPid(0xf055, 0xdf11))
            .manufacturer("Cardona Bits")
            .product("Lightnote")
            .serial_number(serial_string_from_device_id())
            .max_packet_size_0(64)
            .build();

        let (s, r) = make_channel!(u32, MSG_Q_CAPACITY);
        epd_handler::spawn(r).unwrap();

        (
            Shared {
                flash,
            },
            Local {
                delay,
                epd,
                led_b: gpioa.pa8.into_push_pull_output(),
                spi_epd,
                usb_dev,
                webusb,
            },
        )
    }

    #[task(priority = 0, local = [delay, epd, spi_epd])]
    async fn epd_handler(cx: epd_handler::Context, mut receiver: Receiver<'static, u32, MSG_Q_CAPACITY>) {
        rprintln!("epd_handler");
        let epd = cx.local.epd;
        let spi_epd = cx.local.spi_epd;
        let delay = cx.local.delay;
        epd.set_lut(spi_epd, delay, Some(RefreshLut::Full)).unwrap();
        epd.clear_frame(spi_epd, delay).unwrap();
        // epd.update_frame(spi_epd, display.buffer(), delay).unwrap();
        epd.display_frame(spi_epd, delay).unwrap();
    }

    // #[idle]
    // fn idle_task(cx: idle_task::Context) -> ! {
    //     rprintln!("idle");
    //     loop {}
    // }

    #[task(binds = USB, local = [led_b, usb_dev, webusb])]
    fn usb_handler(cx: usb_handler::Context) {
        rprintln!("USB interrupt received.");

        let led: &mut PA8<Output<PushPull>> = cx.local.led_b;
        led.toggle().ok();

        let usb_dev = cx.local.usb_dev;
        usb_dev.poll(&mut [cx.local.webusb]);
    }

    static mut THIS_DEVICE_ID: [u8; 12] = [0u8; 12];
    static mut SERIAL_NUM: [u8; 25] = [0; 25];

    pub fn serial_string_from_device_id() -> &'static str {
        unsafe {
            device_id(&mut THIS_DEVICE_ID);
            return format_no_std::show(&mut SERIAL_NUM, format_args!("{}", THIS_DEVICE_ID.hex()))
                .unwrap();
        }
    }
}
