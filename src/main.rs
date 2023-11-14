#![no_std]
#![no_main]

// pick a panicking behavior
use panic_halt as _; // you can put a breakpoint on `rust_begin_unwind` to catch panics
// use panic_abort as _; // requires nightly
// use panic_itm as _; // logs messages over ITM; requires ITM support
// use panic_semihosting as _; // logs messages to the host stderr; requires a debugger


use stm32l0xx_hal as hal;

#[rtic::app(device = stm32l0xx_hal::pac, dispatchers = [])]
mod app {
    use cortex_m::asm;
    use hex_display::HexDisplayExt;
    use rtt_target::{rtt_init_print, rprintln};
    use usb_device::{
        prelude::{UsbDevice, UsbDeviceBuilder, UsbVidPid},
        bus::UsbBusAllocator,
    };
    use usbd_webusb::{url_scheme, WebUsb};
    use crate::hal::{
        gpio::{gpioa::PA8, Output, PushPull},
        prelude::*,
        rcc::Config,
        signature::device_id,
        syscfg::SYSCFG,
        usb::{USB, UsbBus}
    };

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        led_b: PA8<Output<PushPull>>,
        usb_dev: UsbDevice<'static, UsbBus<USB>>,
        webusb: WebUsb<UsbBus<USB>>
    }

    #[init(local = [USB_BUS: Option<UsbBusAllocator<UsbBus<USB>>> = None])]
    fn init(cx: init::Context) -> (Shared, Local) {
        rtt_init_print!();
        rprintln!("Hello, world!");

        let p = cx.device;
        let mut rcc = p.RCC.freeze(Config::hsi16());
        let mut syscfg = SYSCFG::new(p.SYSCFG, &mut rcc);
        let hsi48 = rcc.enable_hsi48(&mut syscfg, p.CRS);
        let gpioa = p.GPIOA.split(&mut rcc);
        let usb = USB::new(p.USB, gpioa.pa11, gpioa.pa12, hsi48);

        // trick to make usb_bus live forever, lifted from
        // https://github.com/rtic-rs/rtic-examples/blob/master/rtic_v1/stm32f0_hid_mouse/src/main.rs
        let usb_bus = cx.local.USB_BUS;
        *usb_bus = Some(UsbBus::new(usb));
        
        let mut webusb = WebUsb::new(
            usb_bus.as_ref().unwrap(),
            url_scheme::HTTPS,
            "cardonabits.github.io/lightnote-app/",
        );
        let mut usb_dev = UsbDeviceBuilder::new(usb_bus.as_ref().unwrap(), UsbVidPid(0xf055, 0xdf11))
            .manufacturer("Cardona Bits")
            .product("Lightnote")
            .serial_number(serial_string_from_device_id())
            .max_packet_size_0(64)
            .build();
        

        ( Shared {},
          Local { 
            led_b: gpioa.pa8.into_push_pull_output(),
            usb_dev: usb_dev,
            webusb: webusb
            }
        )
    }

    #[task(binds = USB, local = [usb_dev, webusb])]
    fn usb_handler(mut cx: usb_handler::Context) {
        rprintln!("USB interrupt received.");

        let usb_dev = cx.local.usb_dev;
        usb_dev.poll(&mut [cx.local.webusb]);
    }

    #[idle(local = [led_b])]
    fn idle(cx: idle::Context) -> ! {
        let led: &mut PA8<Output<PushPull>> = cx.local.led_b;
        led.set_high().ok();

        //let mut dfu = DFUClass::new(&usb_bus, flash);

        rprintln!("idle");
        loop {
            // Allow MCU to sleep between interrupts
    //        rtic::export::wfi()
        }
    }

    static mut THIS_DEVICE_ID: [u8; 12] = [0u8; 12];
    static mut SERIAL_NUM: [u8; 25] = [0; 25];

    pub fn serial_string_from_device_id() -> &'static str {
        unsafe {
            device_id(&mut THIS_DEVICE_ID);
            return format_no_std::show(
                &mut SERIAL_NUM,
                format_args!("{}", THIS_DEVICE_ID.hex()),
            ).unwrap();
        }
    }
}
