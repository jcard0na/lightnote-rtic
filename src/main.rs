#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

use panic_rtt_target as _;
use defmt_rtt as _;

// mod config;
// mod display;
mod errors;
mod flash;
mod nvm;
mod voltage;

use stm32l0xx_hal as hal;

#[rtic::app(device = stm32l0xx_hal::pac, dispatchers = [RTC])]
mod app {

    const USB_PACKET_SIZE: u16 = 64; // 8,16,32,64

    use crate::{
        // config::{self, FlashConfig, QAType},
        // display::show_q_or_a,
        flash::SpiFlash,
        hal::{
            delay::Delay,
            gpio::{
                gpioa::PA8,
                gpiob::{PB0, PB1, PB2, PB3, PB4, PB5, PB7, PB8, PB9},
                Output, PushPull, OpenDrain,
            },
            i2c::I2c,
            pac::{I2C1},
            prelude::*,
            rcc::Config,
            signature::device_id,
            spi::{Spi, MODE_0},
            syscfg::SYSCFG,
            usb::{UsbBus, USB},
        },
        voltage::VoltageLevels::High,
    };
    use epd_waveshare::{
        epd1in54_v2::{Display1in54, *},
        prelude::*,
    };
    use hex_display::HexDisplayExt;
    use lps22hb::interface::{I2cInterface, i2c::I2cAddress};
    use lps22hb::*;
    use rtic_sync::channel::Receiver;
    use rtic_sync::{channel::*, make_channel};
    // XXX: This should be replaced by shared_bus_rtic
    use shared_bus::{BusManager, NullMutex, SpiProxy};
    use shared_bus_rtic::CommonBus;
    use shtcx::{sensor_class::Sht2Gen, shtc3, ShtCx, PowerMode};
    use usb_device::{
        bus::UsbBusAllocator,
        prelude::{UsbDevice, UsbDeviceBuilder, UsbVidPid},
    };
    use usbd_scsi::Scsi;

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
    struct Shared {}

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
        scsi: Scsi<'static, UsbBus<USB>, SpiFlash<'static>>, 
        sht: ShtCx<Sht2Gen, &'static CommonBus<I2c<I2C1, PB9<Output<OpenDrain>>, PB8<Output<OpenDrain>>>>>,
        spi_epd: SpiProxy<'static, BusMgrInner>,
        usb_dev: UsbDevice<'static, UsbBus<USB>>,
    }

    const MSG_Q_CAPACITY: usize = 1;
    #[init(local = [USB_BUS: Option<UsbBusAllocator<UsbBus<USB>>> = None,
                    SPI_BUS: Option<BusMgr> = None])]
    fn init(cx: init::Context) -> (Shared, Local) {
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
        let scl = gpiob.pb8.into_open_drain_output();
        let sda = gpiob.pb9.into_open_drain_output();

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

        defmt::info!("Setup I2C...");
        // let i2c = p.I2C1.i2c(sda, scl, 100_000.Hz(), &mut rcc); 
        let i2c = I2c::new(p.I2C1, sda, scl, 100_000.Hz(), &mut rcc); 
        let i2c_manager = shared_bus_rtic::new!(i2c,I2c<I2C1, PB9<Output<OpenDrain>>, PB8<Output<OpenDrain>>>);

        let mut sht = shtc3(i2c_manager.acquire());
        // let i2c_interface = I2cInterface::init(i2c_manager.acquire(), I2cAddress::SA0_VCC);
        // let mut lps22hb = LPS22HB::new(i2c_interface);

        // lps22hb.one_shot().unwrap();
        
        // // read temperature and pressure
        
        // let temp = lps22hb.read_temperature().unwrap();                    
        // let press = lps22hb.read_pressure().unwrap();
        // let id = lps22hb.get_device_id().unwrap();

        // Setup EPD
        defmt::info!("Setup EPD...");
        let mut epd =
            Epd1in54::new(&mut spi_epd, cs_epd, busy_in, dc, rst, &mut delay, None).unwrap();
        let mut flash = SpiFlash::new(spi_flash, cs_flash, &mut delay);
        // let mut config = config::FlashConfig::from_flash(&mut flash);

        let scsi: Scsi<'_, UsbBus<USB>, SpiFlash<'_>> = Scsi::new(
            usb_bus.as_ref().unwrap(),
            USB_PACKET_SIZE,
            flash,
            "CardBits",
            "Lightnote",
            "1.0"
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
            Shared {},
            Local {
                delay,
                epd,
                led_b: gpioa.pa8.into_push_pull_output(),
                scsi,
                sht,
                spi_epd,
                usb_dev,
            },
        )
    }

    #[task(priority = 1, local = [delay, epd, sht, spi_epd])]
    async fn epd_handler(
        mut cx: epd_handler::Context,
        mut receiver: Receiver<'static, u32, MSG_Q_CAPACITY>,
    ) {
        defmt::info!("epd_handlerx");
        let epd = cx.local.epd;
        let spi_epd = cx.local.spi_epd;
        let delay = cx.local.delay;
        // let flash = cx.local.flash;

        let sht = cx.local.sht;
        let temperature = sht.measure_temperature(PowerMode::NormalMode,  delay).unwrap();

        // XXX: Until we read it from flash
        // let config = FlashConfig {
        //     page_size: 8192,
        //     num_pages: 100,
        //     q_type: config::QAType::RawImage,
        //     a_type: QAType::Text,
        // };
        // show_q_or_a(
        //     epd,
        //     spi_epd,
        //     High,
        //     flash,
        //     delay,
        //     &config,
        //     0x0000_0000,
        //     false
        // ).ok();
        defmt::info!("epd stuff here...");
        delay.delay_ms(1000u32);
    }

    #[task(binds = USB, priority = 2, local = [led_b, scsi, usb_dev])]
    fn usb_handler(cx: usb_handler::Context) {

        let led = cx.local.led_b;
        led.toggle().ok();

        let usb_dev = cx.local.usb_dev;
        usb_dev.poll(&mut [cx.local.scsi]);
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
