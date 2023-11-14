use cortex_m::prelude::{_embedded_hal_adc_OneShot, _embedded_hal_blocking_delay_DelayMs};
use embedded_graphics::{
    geometry::Point,
    // image::Image,
    pixelcolor::BinaryColor,
    prelude::{DrawTargetExt, Primitive, Size},
    // prelude::{DrawTargetExt},
    primitives::{PrimitiveStyleBuilder, Rectangle},
    Drawable,
};
use epd_waveshare::epd1in54_v2::Display1in54;
use int_enum::IntEnum;
use stm32l0xx_hal::{
    adc::{Adc, Ready, VRef},
    delay::Delay,
    gpio::{
        gpioa::{PA0, PA1, PA4},
        Analog, Output, PushPull,
    },
    prelude::OutputPin,
};
// use tinybmp::Bmp;

#[repr(u32)]
#[derive(PartialEq, PartialOrd, Debug, Clone, Copy, IntEnum)]
pub(crate) enum VoltageLevels {
    Dead = 0,
    Critical = 1,
    VeryLow = 2,
    Low = 3,
    Medium = 4,
    High = 5,
    Full = 6,
}

impl From<VoltageLevels> for &str {
    fn from(charge: VoltageLevels) -> Self {
        match charge {
            VoltageLevels::Full => "Full",
            VoltageLevels::High => "High",
            VoltageLevels::Medium => "Medium",
            VoltageLevels::Low => "Low",
            VoltageLevels::VeryLow => "VeryLow",
            VoltageLevels::Critical => "Critical",
            VoltageLevels::Dead => "Dead",
        }
    }
}

pub(crate) fn charging_levels(solar_intensity: u16) -> VoltageLevels {
    match solar_intensity {
        0..=2700 => VoltageLevels::Dead,
        2701..=3000 => VoltageLevels::Critical,
        3001..=3500 => VoltageLevels::VeryLow,
        3501..=4000 => VoltageLevels::Low,
        4001..=4500 => VoltageLevels::Medium,
        4501..=5000 => VoltageLevels::High,
        5001..=u16::MAX => VoltageLevels::Full,
    }
}

// Show the right icon depending on level of charge
pub(crate) fn draw_charge_icon(charge: &VoltageLevels, display: &mut Display1in54) {
    // Rectangle with 1 pixel wide stroke, filled from left to right top left
    // proportionally to charge
    let style = PrimitiveStyleBuilder::new()
        .stroke_color(BinaryColor::On)
        .stroke_width(1)
        .fill_color(BinaryColor::Off)
        .build();

    Rectangle::new(Point::new(179, 0), Size::new(14, 6))
        .into_styled(style)
        .draw(&mut display.color_converted())
        .unwrap();

    Rectangle::new(Point::new(194, 2), Size::new(2, 2))
        .into_styled(style)
        .draw(&mut display.color_converted())
        .unwrap();

    let length: u32 = 2 * *charge as u32;
    Rectangle::new(Point::new(181, 2), Size::new(length, 2))
        .into_styled(style)
        .draw(&mut display.color_converted())
        .unwrap();
}

// pub(crate) fn draw_charge_icon(charge: &VoltageLevels, display: &mut Display1in54) {
//     let bmp_data;
//     match charge {
//         VoltageLevels::Full => {
//             bmp_data = include_bytes!("../images/sun-charge-32x32-5.bmp");
//         }
//         VoltageLevels::High => {
//             bmp_data = include_bytes!("../images/sun-charge-32x32-4.bmp");
//         }
//         VoltageLevels::Medium => {
//             bmp_data = include_bytes!("../images/sun-charge-32x32-3.bmp");
//         }
//         VoltageLevels::Low => {
//             bmp_data = include_bytes!("../images/sun-charge-32x32-2.bmp");
//         }
//         VoltageLevels::VeryLow => {
//             bmp_data = include_bytes!("../images/sun-charge-32x32-1.bmp");
//         }
//         VoltageLevels::Critical => {
//             bmp_data = include_bytes!("../images/sun-charge-32x32-0.bmp");
//         }
//         VoltageLevels::Dead => {
//             // This level will be displayed as voltage is insufficient to
//             // drive display
//             bmp_data = include_bytes!("../images/sun-charge-32x32-zzz.bmp");
//         }
//     }
//     let bmp = Bmp::<BinaryColor>::from_slice(bmp_data).unwrap();
//     Image::new(&bmp, Point::new(199 - 32, 0))
//         .draw(&mut display.color_converted())
//         .unwrap();
// }

pub(crate) fn read_charge(
    supercap_read_enable: &mut PA4<Output<PushPull>>,
    supercap_in: &mut PA1<Analog>,
    adc: &mut Adc<Ready>,
    delay: &mut Delay,
) -> VoltageLevels {
    supercap_read_enable.set_high().ok();
    delay.delay_ms(50u32);

    // We are reading very high impedance inputs from the supercap
    // and solar voltage dividers, this is why we need very long sample
    // times to get accurate readings
    adc.set_sample_time(stm32l0xx_hal::adc::SampleTime::T_160_5);

    let vdd = read_vdd(adc);
    let scap_in: u16 = adc.read(supercap_in).unwrap();

    supercap_read_enable.set_low().ok();

    let supercap_in = (scap_in as u32 * vdd) / 4095;
    // hprintln!("supercap_in = {}.{:03}V", supercap_in / 1000, supercap_in % 1000).ok();

    // Calculate supercap voltage from the voltage divider.
    let supercap_voltage = 2 * supercap_in;

    // hprintln!("supercap voltage ={}.{:03}V", supercap_voltage/1000, supercap_voltage % 1000).ok();
    let charge = charging_levels(supercap_voltage as u16);

    charge
}

fn read_vdd(adc: &mut Adc<Ready>) -> u32 {
    // Read the V_REFINT ADC channel.  Should be close to the one stored during
    // calibration in VREF_CAL_ADDRESS
    let vref: u16 = adc.read(&mut VRef).unwrap();
    // Per datasheet, this is where the ADC reading for VREFINT_CAL voltage is stored
    const VREF_CAL_ADDRESS: *mut u16 = 0x1FF8_0078 as *mut u16;
    let vref_cal: u16;
    unsafe {
        vref_cal = *VREF_CAL_ADDRESS;
    }

    // Calculate ADC scaling factor in current conditions
    // See RM0376 14.9 "Calculating the actual VDDA voltage using the
    // internal reference voltage"

    // Given in datasheet, the VDD that was using during calibration
    // Note: not using floating point or else all floating point functions will
    // be pulled in which will dramatically increase the size of the binary
    const VDDA_CARAC: u32 = 3_000;

    // this is our Vdd based on the internal Vref reading
    let vdd = (VDDA_CARAC * vref_cal as u32) / vref as u32;
    // hprintln!("calculated our Vdd = {}.{:03}V", vdd / 1000, vdd % 1000).ok();

    vdd
}

pub(crate) fn read_solar(solar_in: &mut PA0<Analog>, adc: &mut Adc<Ready>) -> VoltageLevels {
    // We are reading very high impedance inputs from the supercap
    // and solar voltage dividers, this is why we need very long sample
    // times to get accurate readings
    adc.set_sample_time(stm32l0xx_hal::adc::SampleTime::T_160_5);

    let solar: u16 = adc.read(solar_in).unwrap();

    let vdd = read_vdd(adc);
    // hprintln!("solar_adc={}", solar).ok();
    let solar = (solar as u32 * vdd) / 4095;
    // hprintln!("solar_in={}.{:03}V", solar/1000, solar % 1000).ok();

    let solar = 2 * solar;

    // hprintln!("solar={}.{:03}V", solar/1000, solar % 1000).ok();
    let solar = charging_levels(solar as u16);
    // hprintln!("solar={:?}", solar).ok();
    solar
}
