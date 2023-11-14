use core::ptr;
use int_enum::IntEnum;
use stm32l0xx_hal::{
    flash::{EEPROM_START_BANK1, FLASH},
    pac,
    rcc::Rcc,
};

use crate::voltage::VoltageLevels;

#[derive(Copy, Clone)]
enum NvmVariableNames {
    WakeUpReason = 0x4,
    VoltageLevel = 0x8,
    DisplayAddress = 0xc,
    AnswerPending = 0x10,
}

pub enum Error {
    InvalidAddress,
}

#[repr(u32)]
#[derive(PartialEq, Debug, Clone, Copy, IntEnum)]
pub enum WakeUpReasons {
    // We don't want to use zero as a value as it could be confused with
    // uninitialized memory.
    ButtonPress = 1,
    RtcTimeout = 2,
    ChargingStartedEvent = 3,
    AccelerometerEvent = 5,
    SomeOtherWeirdEvent = 6,
}

pub struct Nvm {
    nvm: FLASH,
}

impl Nvm {
    pub(crate) fn new(nvm: pac::FLASH, rcc: &mut Rcc) -> Self {
        let nvm = FLASH::new(nvm, rcc);
        Self { nvm }
    }

    pub(crate) fn save_wakeup_reason(self: &mut Self, reason: WakeUpReasons) {
        let address = (EEPROM_START_BANK1 + NvmVariableNames::WakeUpReason as usize) as *mut u32;
        self.nvm
            .write_word(address, reason as u32)
            .expect("Failed to write to EEPROM");
    }

    pub(crate) fn read_wakeup_reason(self: &Self) -> WakeUpReasons {
        let address = (EEPROM_START_BANK1 + NvmVariableNames::WakeUpReason as usize) as *mut u32;
        let val = unsafe { *address };
        let reason = WakeUpReasons::from_int(val);
        if reason.is_err() {
            WakeUpReasons::SomeOtherWeirdEvent
        } else {
            reason.unwrap()
        }
    }

    pub(crate) fn save_charge_level(self: &mut Self, charge: VoltageLevels) {
        let address = (EEPROM_START_BANK1 + NvmVariableNames::VoltageLevel as usize) as *mut u32;
        self.nvm
            .write_word(address, charge as u32)
            .expect("Failed to write to EEPROM");
    }

    pub(crate) fn read_charge_level(self: &Self) -> VoltageLevels {
        let address = (EEPROM_START_BANK1 + NvmVariableNames::VoltageLevel as usize) as *mut u32;
        let val = unsafe { *address };
        let reason = VoltageLevels::from_int(val);
        if reason.is_err() {
            VoltageLevels::Critical
        } else {
            reason.unwrap()
        }
    }

    pub(crate) fn save_display_addr(self: &mut Self, disp_addr: u32) {
        let address = (EEPROM_START_BANK1 + NvmVariableNames::DisplayAddress as usize) as *mut u32;
        self.nvm
            .write_word(address, disp_addr)
            .expect("Failed to write to EEPROM");
    }

    pub(crate) fn read_disp_addr(self: &Self) -> Option<u32> {
        let address = (EEPROM_START_BANK1 + NvmVariableNames::DisplayAddress as usize) as *mut u32;
        let val = unsafe { *address };
        if val != 0xffff_ffff {
            Some(val)
        } else {
            None
        }
    }

    pub(crate) fn save_answer_pending(self: &mut Self, answer_pending: bool) {
        let address = (EEPROM_START_BANK1 + NvmVariableNames::AnswerPending as usize) as *mut u32;
        self.nvm
            .write_word(address, answer_pending as u32)
            .expect("Failed to write to EEPROM");
    }

    pub(crate) fn read_answer_pending(self: &Self) -> bool {
        let address = (EEPROM_START_BANK1 + NvmVariableNames::AnswerPending as usize) as *mut u32;
        let val = unsafe { *address };
        val != 0
    }

    pub(crate) fn read_raw(
        self: &Self,
        buf: &mut [u8],
        offset: u32,
        length: usize,
    ) -> Result<(), Error> {
        const EEPROM_SIZE_IN_BYTES: u32 = 2_048;
        if offset + length as u32 > EEPROM_SIZE_IN_BYTES {
            return Err(Error::InvalidAddress);
        }
        let address = (EEPROM_START_BANK1 as u32 + offset) as *mut u8;
        unsafe {
            ptr::copy(address, buf.as_mut_ptr() as *mut u8, length);
        }

        Ok(())
    }
}
