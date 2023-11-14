use hex_display::HexDisplayExt;
use stm32l0xx_hal::{prelude::OutputPin, signature::device_id};
use usb_device::prelude::{UsbDeviceBuilder, UsbVidPid};
use usbd_dfu::DFUClass;
use usbd_webusb::{url_scheme, WebUsb};

use crate::{
    flash::SpiFlash,
    hal::{
        gpio::{gpioa::PA8, Analog},
        prelude::ToggleableOutputPin,
        usb::{UsbBus, USB},
    },
};

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

pub(crate) fn config_poll(usb: USB, pa8: PA8<Analog>, flash: SpiFlash) -> bool {
    let mut led = pa8.into_push_pull_output();
    let usb_bus = UsbBus::new(usb);
    let mut dfu = DFUClass::new(&usb_bus, flash);
    let mut webusb = WebUsb::new(
        &usb_bus,
        url_scheme::HTTPS,
        "cardonabits.github.io/lightnote-app/",
    );
    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0xf055, 0xdf11))
        .manufacturer("Cardona Bits")
        .product("Lightnote")
        .serial_number(serial_string_from_device_id())
        .max_packet_size_0(64)
        .build();

    let mut attached_to_host = false;
    const NUMBER_OF_POLL_CALLS: usize = 100000;
    for _ in 0..NUMBER_OF_POLL_CALLS {
        if usb_dev.poll(&mut [&mut dfu, &mut webusb]) {
            // only light up led if there was data exchanged with the host
            attached_to_host = true;
        }
    }
    led.set_low().ok();

    if attached_to_host {
        loop {
            led.toggle().ok();
            if usb_dev.poll(&mut [&mut dfu, &mut webusb]) {
                //
            }
        }
    }
    led.set_low().ok();

    attached_to_host
}
