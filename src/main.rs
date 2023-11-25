#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

use panic_rtt_target as _;

mod config;
mod display;
mod errors;
mod flash;
mod nvm;
mod voltage;

use stm32l0xx_hal as hal;

#[derive(Default)]
struct State {
    storage_offset: usize,
    sense_key: Option<u8>,
    sense_key_code: Option<u8>,
    sense_qualifier: Option<u8>,
}

impl State {
    fn reset(&mut self) {
        self.storage_offset = 0;
        self.sense_key = None;
        self.sense_key_code = None;
        self.sense_qualifier = None;
    }
}

#[rtic::app(device = stm32l0xx_hal::pac, dispatchers = [RTC])]
mod app {

    use core::mem::MaybeUninit;

    const USB_PACKET_SIZE: u16 = 64; // 8,16,32,64
    static mut USB_TRANSPORT_BUF: MaybeUninit<[u8; 512]> = MaybeUninit::uninit();
    const BLOCK_SIZE: u32 = 512;
    // this needs to match the size of disk.img
    const BLOCKS: u32 = 16;
    const MAX_LUN: u8 = 0; // max 0x0F
    // XXX: This is the actual file content.  It needs to be replaced by flash
    static mut STORAGE: [u8; 8192] = *include_bytes!("disk.img");

    static mut STATE: State = State {
        storage_offset: 0,
        sense_key: None,
        sense_key_code: None,
        sense_qualifier: None,
    };

    use super::*;
    use crate::{
        config::{self, FlashConfig, QAType},
        display::show_q_or_a,
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
    use rtt_target::{rprintln, rtt_init_print};
    // XXX: This should be replaced by shared_bus_rtic
    use shared_bus::{BusManager, NullMutex, SpiProxy};
    use shared_bus_rtic::CommonBus;
    use shtcx::{sensor_class::Sht2Gen, shtc3, ShtCx, PowerMode};
    use usb_device::{
        bus::UsbBusAllocator,
        prelude::{UsbDevice, UsbDeviceBuilder, UsbDeviceState, UsbVidPid},
    };
    use usbd_storage::subclass::scsi::{Scsi, ScsiCommand};
    use usbd_storage::subclass::Command;
    use usbd_storage::transport::bbb::{BulkOnly, BulkOnlyError};
    use usbd_storage::transport::TransportError;
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
        flash: SpiFlash<'static>,
        led_b: PA8<Output<PushPull>>,
        scsi: Scsi<BulkOnly<'static, UsbBus<USB>, &'static mut [u8]>>,
        sht: ShtCx<Sht2Gen, &'static CommonBus<I2c<I2C1, PB9<Output<OpenDrain>>, PB8<Output<OpenDrain>>>>>,
        spi_epd: SpiProxy<'static, BusMgrInner>,
        usb_dev: UsbDevice<'static, UsbBus<USB>>,
        webusb: WebUsb<UsbBus<USB>>,
    }

    const MSG_Q_CAPACITY: usize = 1;
    #[init(local = [USB_BUS: Option<UsbBusAllocator<UsbBus<USB>>> = None,
                    SPI_BUS: Option<BusMgr> = None])]
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

        rprintln!("Setup I2C...");
        let i2c = p.I2C1.i2c(sda, scl, 100_000.Hz(), &mut rcc); 
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

        let mut scsi = usbd_storage::subclass::scsi::Scsi::new(
            usb_bus.as_ref().unwrap(),
            USB_PACKET_SIZE,
            MAX_LUN,
            unsafe { USB_TRANSPORT_BUF.assume_init_mut().as_mut_slice() },
        )
        .unwrap();

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
                flash,
                led_b: gpioa.pa8.into_push_pull_output(),
                scsi,
                sht,
                spi_epd,
                usb_dev,
                webusb,
            },
        )
    }

    #[task(priority = 1, local = [delay, epd, flash, sht, spi_epd])]
    async fn epd_handler(
        mut cx: epd_handler::Context,
        mut receiver: Receiver<'static, u32, MSG_Q_CAPACITY>,
    ) {
        rprintln!("epd_handlerx");
        let epd = cx.local.epd;
        let spi_epd = cx.local.spi_epd;
        let delay = cx.local.delay;
        let flash = cx.local.flash;

        let sht = cx.local.sht;
        let temperature = sht.measure_temperature(PowerMode::NormalMode, delay).unwrap();

        // XXX: Until we read it from flash
        let config = FlashConfig {
            page_size: 8192,
            num_pages: 100,
            q_type: config::QAType::RawImage,
            a_type: QAType::Text,
        };
        show_q_or_a(
            epd,
            spi_epd,
            High,
            flash,
            delay,
            &config,
            0x0000_0000,
            false
        ).ok();
        rprintln!("epd stuff here...");
        delay.delay_ms(1000u32);
    }

    #[task(binds = USB, priority = 2, local = [led_b, scsi, usb_dev, webusb])]
    fn usb_handler(cx: usb_handler::Context) {
        rprintln!("USB interrupt received.");

        let led = cx.local.led_b;
        led.toggle().ok();

        let usb_dev = cx.local.usb_dev;
        if !usb_dev.poll(&mut [cx.local.webusb, cx.local.scsi]) {
            // No USB work is required
            return;
        }

        // clear state if just configured or reset
        if matches!(usb_dev.state(), UsbDeviceState::Default) {
            unsafe {
                STATE.reset();
            };
        }

        let _ = cx.local.scsi.poll(|command| {
            led.set_low();
            if let Err(err) = process_scsi_command(command) {
                rprintln!("scsi poll error");
            }
        });
        rprintln!("USB interrupt done");
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

    fn process_scsi_command(
        mut command: Command<ScsiCommand, Scsi<BulkOnly<UsbBus<USB>, &mut [u8]>>>,
    ) -> Result<(), TransportError<BulkOnlyError>> {
        rprintln!("Handling: scsi cmd");

        match command.kind {
            ScsiCommand::TestUnitReady { .. } => {
                command.pass();
            }
            ScsiCommand::Inquiry { .. } => {
                command.try_write_data_all(&[
                    0x00, // periph qualifier, periph device type
                    0x80, // Removable
                    0x04, // SPC-2 compliance
                    0x02, // NormACA, HiSu, Response data format
                    0x20, // 36 bytes in total
                    0x00, // additional fields, none set
                    0x00, // additional fields, none set
                    0x00, // additional fields, none set
                    b'U', b'N', b'K', b'N', b'O', b'W', b'N', b' ', // 8-byte T-10 vendor id
                    b'S', b'T', b'M', b'3', b'2', b' ', b'U', b'S', b'B', b' ', b'F', b'l', b'a',
                    b's', b'h', b' ', // 16-byte product identification
                    b'1', b'.', b'2', b'3', // 4-byte product revision
                ])?;
                command.pass();
            }
            ScsiCommand::RequestSense { .. } => unsafe {
                command.try_write_data_all(&[
                    0x70,                         // RESPONSE CODE. Set to 70h for information on current errors
                    0x00,                         // obsolete
                    STATE.sense_key.unwrap_or(0), // Bits 3..0: SENSE KEY. Contains information describing the error.
                    0x00,
                    0x00,
                    0x00,
                    0x00, // INFORMATION. Device-specific or command-specific information.
                    0x00, // ADDITIONAL SENSE LENGTH.
                    0x00,
                    0x00,
                    0x00,
                    0x00,                               // COMMAND-SPECIFIC INFORMATION
                    STATE.sense_key_code.unwrap_or(0),  // ASC
                    STATE.sense_qualifier.unwrap_or(0), // ASCQ
                    0x00,
                    0x00,
                    0x00,
                    0x00,
                ])?;
                STATE.reset();
                command.pass();
            },
            ScsiCommand::ReadCapacity10 { .. } => {
                let mut data = [0u8; 8];
                let _ = &mut data[0..4].copy_from_slice(&u32::to_be_bytes(BLOCKS - 1));
                let _ = &mut data[4..8].copy_from_slice(&u32::to_be_bytes(BLOCK_SIZE));
                command.try_write_data_all(&data)?;
                command.pass();
            }
            ScsiCommand::ReadCapacity16 { .. } => {
                let mut data = [0u8; 16];
                let _ = &mut data[0..8].copy_from_slice(&u32::to_be_bytes(BLOCKS - 1));
                let _ = &mut data[8..12].copy_from_slice(&u32::to_be_bytes(BLOCK_SIZE));
                command.try_write_data_all(&data)?;
                command.pass();
            }
            ScsiCommand::ReadFormatCapacities { .. } => {
                let mut data = [0u8; 12];
                let _ = &mut data[0..4].copy_from_slice(&[
                    0x00, 0x00, 0x00, 0x08, // capacity list length
                ]);
                let _ = &mut data[4..8].copy_from_slice(&u32::to_be_bytes(BLOCKS as u32)); // number of blocks
                data[8] = 0x01; //unformatted media
                let block_length_be = u32::to_be_bytes(BLOCK_SIZE);
                data[9] = block_length_be[1];
                data[10] = block_length_be[2];
                data[11] = block_length_be[3];

                command.try_write_data_all(&data)?;
                command.pass();
            }
            ScsiCommand::Read { lba, len } => unsafe {
                let lba = lba as u32;
                let len = len as u32;
                if STATE.storage_offset != (len * BLOCK_SIZE) as usize {
                    let start = (BLOCK_SIZE * lba) as usize + STATE.storage_offset;
                    let end = (BLOCK_SIZE * lba) as usize + (BLOCK_SIZE * len) as usize;

                    // Uncomment this in order to push data in chunks smaller than a USB packet.
                    let end = core::cmp::min(start + USB_PACKET_SIZE as usize - 1, end);
                    rprintln!("Data transfer >>>>>>>>");
                    let count = command.write_data(&mut STORAGE[start..end])?;
                    STATE.storage_offset += count;
                } else {
                    command.pass();
                    STATE.storage_offset = 0;
                }
            },
            ScsiCommand::Write { lba, len } => unsafe {
                let lba = lba as u32;
                let len = len as u32;
                if STATE.storage_offset != (len * BLOCK_SIZE) as usize {
                    let start = (BLOCK_SIZE * lba) as usize + STATE.storage_offset;
                    let end = (BLOCK_SIZE * lba) as usize + (BLOCK_SIZE * len) as usize;
                    rprintln!("Data transfer <<<<<<<<");
                    let count = command.read_data(&mut STORAGE[start..end])?;
                    STATE.storage_offset += count;

                    if STATE.storage_offset == (len * BLOCK_SIZE) as usize {
                        command.pass();
                        STATE.storage_offset = 0;
                    }
                } else {
                    command.pass();
                    STATE.storage_offset = 0;
                }
            },
            ScsiCommand::ModeSense6 { .. } => {
                command.try_write_data_all(&[
                    0x03, // number of bytes that follow
                    0x00, // the media type is SBC
                    0x00, // not write-protected, no cache-control bytes support
                    0x00, // no mode-parameter block descriptors
                ])?;
                command.pass();
            }
            ScsiCommand::ModeSense10 { .. } => {
                command.try_write_data_all(&[0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])?;
                command.pass();
            }
            ref unknown_scsi_kind => {
                rprintln!("Unknown SCSI command");
                unsafe {
                    STATE.sense_key.replace(0x05); // illegal request Sense Key
                    STATE.sense_key_code.replace(0x20); // Invalid command operation ASC
                    STATE.sense_qualifier.replace(0x00); // Invalid command operation ASCQ
                }
                command.fail();
            }
        }

        Ok(())
    }
}
