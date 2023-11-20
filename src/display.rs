use core::cmp::max;

use embedded_graphics::{
    geometry::Point,
    image::{Image, ImageRaw},
    Drawable,
};
use embedded_graphics::{
    pixelcolor::BinaryColor,
    prelude::{DrawTargetExt, Size},
    primitives::{Primitive, PrimitiveStyleBuilder, Rectangle},
};

use rtt_target::rprintln;
use shared_bus::{NullMutex, SpiProxy};
use static_assertions as sa;

use stm32l0xx_hal::{
    delay::Delay,
    gpio::gpiob::{PB0, PB1, PB2, PB3, PB4, PB5, PB7},
};

use u8g2_fonts::{
    fonts,
    types::{FontColor, HorizontalAlignment, VerticalPosition},
    FontRenderer,
};

use usbd_dfu::{DFUMemError, DFUMemIO};

use crate::flash::SpiFlash;

// For GDE015OC1 use:
// use epd_waveshare::{epd1in54::*, prelude::*};
// For GDEH0154D67 use:
use epd_waveshare::{epd1in54_v2::*, prelude::*};

use crate::{
    config::{FlashConfig, QAType},
    errors::LightNoteErrors,
    voltage::{draw_charge_icon, VoltageLevels},
};

type Epd<'a> = epd_waveshare::epd1in54_v2::Epd1in54<
    SpiProxy<
        'a,
        NullMutex<
            stm32l0xx_hal::spi::Spi<
                stm32l0xx_hal::pac::SPI1,
                (
                    PB3<stm32l0xx_hal::gpio::Analog>,
                    PB4<stm32l0xx_hal::gpio::Analog>,
                    PB5<stm32l0xx_hal::gpio::Analog>,
                ),
            >,
        >,
    >,
    PB2<stm32l0xx_hal::gpio::Output<stm32l0xx_hal::gpio::PushPull>>,
    PB7<stm32l0xx_hal::gpio::Input<stm32l0xx_hal::gpio::Floating>>,
    PB1<stm32l0xx_hal::gpio::Output<stm32l0xx_hal::gpio::PushPull>>,
    PB0<stm32l0xx_hal::gpio::Output<stm32l0xx_hal::gpio::PushPull>>,
    stm32l0xx_hal::delay::Delay,
>;
type SpiEpd<'a> = SpiProxy<
    'a,
    NullMutex<
        stm32l0xx_hal::spi::Spi<
            stm32l0xx_hal::pac::SPI1,
            (
                PB3<stm32l0xx_hal::gpio::Analog>,
                PB4<stm32l0xx_hal::gpio::Analog>,
                PB5<stm32l0xx_hal::gpio::Analog>,
            ),
        >,
    >,
>;

impl From<DFUMemError> for LightNoteErrors {
    fn from(_: DFUMemError) -> Self {
        LightNoteErrors::FailedToReadFromFlash
    }
}

#[derive(Debug)]
pub(crate) enum QAStatus {
    AnswerPending,
    ReadyForNextQuestion,
}

pub(crate) fn show_q_or_a<'a>(
    epd: &mut Epd<'a>,
    spi_epd: &mut SpiEpd<'a>,
    charge: VoltageLevels,
    flash: &mut SpiFlash,
    delay: &mut Delay,
    config: &FlashConfig,
    display_addr: u32,
    show_answer: bool,
) -> Result<QAStatus, LightNoteErrors> {

    rprintln!("show_q_or_a");
    let mut status = QAStatus::ReadyForNextQuestion;

    // Use display graphics from embedded-graphics
    let mut display = Display1in54::default();

    // // Display1in54 internal buffer is initialized black.  We want it white.
    // Rectangle::new(Point::new(0, 0), Size::new(200, 200))
    //     .into_styled(
    //         PrimitiveStyleBuilder::new()
    //             .stroke_width(0)
    //             .fill_color(Color::White)
    //             .build(),
    //     )
    //     .draw(&mut display)
    //     .unwrap();

    // flash.check_flash_id()?;

    // const LINE_HEIGHT: u32 = 22;
    // let font = FontRenderer::new::<fonts::u8g2_font_helvB12_te>()
    //     .with_ignore_unknown_chars(true)
    //     .with_line_height(LINE_HEIGHT);

    // const READ_BUFFER_SIZE: usize = 1000;
    // // READ_BUFFER_SIZE must be a divisor of 5000 so that we read the entire data
    // // READ_BUFFER_SIZE must be a multiple of 25, which is the data length in bytes
    // // of a single row
    // sa::const_assert_eq!(RAW_IMAGE_SIZE % READ_BUFFER_SIZE as u32, 0);
    // sa::const_assert_eq!(READ_BUFFER_SIZE as u32 % 25, 0);
    // const RAW_IMAGE_SIZE: u32 = 5000;
    // const MEM_READS_PER_IMAGE: u32 = RAW_IMAGE_SIZE / (READ_BUFFER_SIZE as u32);
    // const IMAGE_ROWS_PER_READ: u32 = READ_BUFFER_SIZE as u32 / 25;
    // let mut addr;
    // if config.q_type == QAType::RawImage && !show_answer {
    //     addr = display_addr;
    //     for i in 0u32..MEM_READS_PER_IMAGE {
    //         let buf = flash.read(addr, READ_BUFFER_SIZE)?;

    //         let raw_image = ImageRaw::<BinaryColor>::new(&buf[..], 200);
    //         let image = Image::new(&raw_image, Point::new(0, (i * IMAGE_ROWS_PER_READ) as i32));
    //         if let Err(_) = image.draw(&mut display.color_converted()) {
    //             return Err(LightNoteErrors::FailedToRenderImage);
    //         }
    //         addr += READ_BUFFER_SIZE as u32;
    //     }
    //     if config.a_type == QAType::Text {
    //         status = QAStatus::AnswerPending;
    //     }
    // }
    // if show_answer && config.a_type == QAType::Text {
    //      addr = display_addr + RAW_IMAGE_SIZE;
    //      let buf = flash.read(addr, READ_BUFFER_SIZE)?;
    //      let mut iter = buf.split(|b| *b == 0u8);
    //      if let Some(text_buffer) = iter.next() {
    //         if let Ok(text) = core::str::from_utf8(text_buffer) {
    //             let c = text.matches("\n").count() as i32;
    //             let text_origin = Point::new(100, max(0, 100 - LINE_HEIGHT as i32 * (c - 1) / 2));
    //             if let Err(_) = font.render_aligned(
    //                 text,
    //                 text_origin,
    //                 VerticalPosition::Baseline,
    //                 HorizontalAlignment::Center,
    //                 FontColor::Transparent(Color::Black),
    //                 &mut display,
    //             ) {
    //                 return Err(LightNoteErrors::FailedToRenderText);
    //             }
    //         }
    //      }
    // }
    // if let Some(charge) = charge_to_show_for(charge) {
    //     draw_charge_icon(&charge, &mut display);
    // }

    epd.set_lut(spi_epd, delay, Some(RefreshLut::Full)).unwrap();
    //epd.update_frame(spi_epd, display.buffer(), delay).unwrap();
    epd.display_frame(spi_epd, delay).unwrap();
    Ok(status)
}

pub(crate) fn charge_to_show_for(charge: VoltageLevels) -> Option<VoltageLevels> {
    // Charge indicator distracts, only show it if voltage is low
    if charge <= VoltageLevels::Low {
        Some(charge)
    } else {
        None
    }
}
