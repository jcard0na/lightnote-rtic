use cortex_m::prelude::_embedded_hal_blocking_delay_DelayMs;
use stm32l0xx_hal::{
    delay::Delay,
    gpio::{Output, Pin, PushPull},
    prelude::OutputPin,
};

use crate::config::FlashConfigError;

#[derive(Clone, Copy)]
pub(super) enum LightNoteErrors {
    FailedToVerifyAccelConfig = 12,
    InvalidFlashConfigMagicId = 32,
    InvalidQAType = 33,
    FailedToReadFlashID = 37,
    // FailedToInitializeFlash = 38,
    AwakenedByUnexpectedEvent = 41,
    FailedToRenderText = 44,
    FailedToRenderImage = 45,
    // FailedToReadOrientation = 66,
    FailedToReadFromFlash = 73,
}

impl From<FlashConfigError> for LightNoteErrors {
    fn from(fc_error: FlashConfigError) -> Self {
        match fc_error {
            FlashConfigError::InvalidFlashConfigMagicId => {
                LightNoteErrors::InvalidFlashConfigMagicId
            }
            FlashConfigError::InvalidQAType => LightNoteErrors::InvalidQAType,
            FlashConfigError::FailedToReadFlash => LightNoteErrors::FailedToReadFromFlash,
        }
    }
}

#[allow(dead_code)]
pub(super) fn raise(
    error: LightNoteErrors,
    led1: &mut Pin<Output<PushPull>>,
    led10: &mut Pin<Output<PushPull>>,
    delay: &mut Delay,
) {
    let tens = (error as u8) / 10;
    let fifties = tens / 5;
    let tens = tens % 5;
    for _ in 0..fifties {
        led10.set_high().ok();
        delay.delay_ms(2_000u32);
        led10.set_low().ok();
        delay.delay_ms(500u32);
    }

    for _ in 0..tens {
        led10.set_high().ok();
        delay.delay_ms(500u32);
        led10.set_low().ok();
        delay.delay_ms(500u32);
    }
    let ones = (error as u8) % 10;
    let fives = ones / 5;
    let ones = ones % 5;
    for _ in 0..fives {
        led1.set_high().ok();
        delay.delay_ms(2_000u32);
        led1.set_low().ok();
        delay.delay_ms(500u32);
    }

    for _ in 0..ones {
        led1.set_high().ok();
        delay.delay_ms(500u32);
        led1.set_low().ok();
        delay.delay_ms(500u32);
    }
    delay.delay_ms(5_000u32);
}
