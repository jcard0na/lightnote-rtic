use core::{convert::TryInto, fmt::Display};

use cortex_m_semihosting::hprintln;
use usbd_dfu::{DFUMemError, DFUMemIO};

use crate::flash::SpiFlash;

#[derive(Debug, PartialEq)]
pub(crate) enum QAType {
    MonospaceText = 0,
    Text = 1,
    RawImage = 2,
}

#[derive(Debug)]
pub(crate) enum FlashConfigError {
    InvalidFlashConfigMagicId,
    InvalidQAType,
    FailedToReadFlash,
}

const CONFIG_SECTOR_ADDRESS: u32 = 0xff_f000;
const MAGIC_ID_OFFSET: usize = 0x0;
const PAGE_SIZE_OFFSET: usize = 0x4;
const NUM_PAGES_OFFSET: usize = 0x6;
const TYPE_Q_OFFSET: usize = 0xa;
const TYPE_A_OFFSET: usize = 0xb;
const CONFIG_SIZE: usize = 0xc;

#[derive(Debug)]
pub(crate) struct FlashConfig {
    pub(crate) page_size: u16,
    pub(crate) num_pages: u32,
    pub(crate) q_type: QAType,
    pub(crate) a_type: QAType,
}

impl TryInto<QAType> for u8 {
    type Error = FlashConfigError;

    fn try_into(self) -> Result<QAType, Self::Error> {
        match self {
            0 => Ok(QAType::MonospaceText),
            1 => Ok(QAType::Text),
            2 => Ok(QAType::RawImage),
            _ => Err(FlashConfigError::InvalidQAType),
        }
    }
}

impl Default for FlashConfig {
    // pick a safe default: at worst we'll display
    // random noise on the display corresponding to
    // address 0x0
    fn default() -> Self {
        Self {
            page_size: 8192,
            num_pages: 1,
            q_type: QAType::RawImage,
            a_type: QAType::RawImage,
        }
    }
}

impl Display for QAType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            QAType::MonospaceText => write!(f, "monospace"),
            QAType::Text => write!(f, "text"),
            QAType::RawImage => write!(f, "image"),
        }
    }
}

impl From<DFUMemError> for FlashConfigError {
    fn from(_: DFUMemError) -> Self {
        FlashConfigError::FailedToReadFlash
    }
}

impl FlashConfig {
    pub(crate) fn from_flash(flash: &mut SpiFlash) -> Result<Self, FlashConfigError> {
        let addr = CONFIG_SECTOR_ADDRESS + MAGIC_ID_OFFSET as u32;
        // let buf = flash.read(addr, CONFIG_SIZE)?;
        let magic_id =
            u32::from_le_bytes(buf[MAGIC_ID_OFFSET..PAGE_SIZE_OFFSET].try_into().unwrap());
        if magic_id != 0x23571113 {
            return Err(FlashConfigError::InvalidFlashConfigMagicId);
        }
        let page_size =
            u16::from_le_bytes(buf[PAGE_SIZE_OFFSET..NUM_PAGES_OFFSET].try_into().unwrap());
        let num_pages =
            u32::from_le_bytes(buf[NUM_PAGES_OFFSET..TYPE_Q_OFFSET].try_into().unwrap());
        let q_type: Result<QAType, FlashConfigError> =
            u8::from_le_bytes(buf[TYPE_Q_OFFSET..TYPE_A_OFFSET].try_into().unwrap()).try_into();
        if q_type.is_err() {
            return Err(q_type.unwrap_err());
        }
        let q_type = q_type.unwrap();

        let a_type: Result<QAType, FlashConfigError> =
            u8::from_le_bytes(buf[TYPE_A_OFFSET..CONFIG_SIZE].try_into().unwrap()).try_into();
        if a_type.is_err() {
            return Err(a_type.unwrap_err());
        }
        let a_type = a_type.unwrap();

        Ok(FlashConfig {
            page_size,
            num_pages,
            q_type,
            a_type,
        })
    }
}

#[allow(dead_code)]
pub(crate) fn dump(config: &FlashConfig) {
    hprintln!("page_size: {}", config.page_size).ok();
    hprintln!("num_pages: {}", config.num_pages).ok();
    hprintln!("q type: {}", config.q_type).ok();
    hprintln!("a type: {}", config.a_type).ok();
}
