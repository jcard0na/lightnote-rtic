// use core::sync::atomic::AtomicU32;
use cortex_m::prelude::_embedded_hal_blocking_delay_DelayMs;
// use cortex_m_semihosting::hprintln;
use shared_bus::{NullMutex, SpiProxy};
use spi_memory::{series25::Flash, BlockDevice, Read};
use stm32l0xx_hal::{
    delay::Delay,
    gpio::{
        gpiob::{PB3, PB4, PB5, PB6},
        Output, PushPull,
    },
    pac,
    prelude::OutputPin,
    rcc::{Config, RccExt},
};

use usbd_dfu::{DFUManifestationError, DFUMemError, DFUMemIO};

use crate::{
    errors::LightNoteErrors,
    nvm::{self, Nvm},
};

// pub(crate) static DISPLAY_ADDRESS: AtomicU32 = AtomicU32::new(0xffff_ffff);

// Address where EEPROM is mapped
const EEPROM_VIRTUAL_ADDRESS: u32 = 0x100_0000;
const VERSION_VIRTUAL_ADDRESS: u32 = 0x100_0800;
const CONTROL_AREA_ADDRESS: u32 = 0x100_0c00;

impl From<nvm::Error> for DFUMemError {
    fn from(value: nvm::Error) -> Self {
        match value {
            nvm::Error::InvalidAddress => DFUMemError::Address,
        }
    }
}

impl DFUMemIO for SpiFlash<'_> {
    // The first 4096 in MEM_INFO_STRING below is the number of sectors, and
    // the second 4096 is the sector size.  Keep that in mind if you change the
    // format of the flash chip.  The sector size affects the granularity of
    // erase operations.
    // The memory layout consists of three regions, 16MB of flash, 2K of
    // EEPROM for which we only implement read-only access for debugging and
    // 1K for version string and other static content.
    const MEM_INFO_STRING: &'static str = "@Flash/0x00000000/4096*4Kg,1*2Ka,1*1Ka,1*4d";
    const INITIAL_ADDRESS_POINTER: u32 = 0x0;
    // The dfu host side will check after these times the status of the
    // respective memory operations.  While the operation is in progress,
    // we block, so the status cannot be reported.  Therefore, the times
    // here need to be longer than the actual operations or else the host
    // will report an error.

    // Datasheet gives worst case time is 3ms per (256 B) Page Program, and we
    // have TRANSFER_SIZE/256 operations per transfer.
    // Increment this if dfu-util reports GET_STATUS error
    const PROGRAM_TIME_MS: u32 = 120;
    // Datasheet gives worst case time is 400ms per 4k sector erase command.
    const ERASE_TIME_MS: u32 = 400;
    // Datasheet gives worst case time is 200s per full chip erase command.
    const FULL_ERASE_TIME_MS: u32 = 200_000;
    const TRANSFER_SIZE: u16 = 1024;

    fn read(&mut self, address: u32, length: usize) -> Result<&[u8], DFUMemError> {

        if address == VERSION_VIRTUAL_ADDRESS {
            let version = env!("VERGEN_GIT_SHA").as_bytes();
            self.buffer[0..version.len()].copy_from_slice(version);
            return Ok(&self.buffer[0..length]);
        }
        if address >= EEPROM_VIRTUAL_ADDRESS && address < VERSION_VIRTUAL_ADDRESS {
            // reading EEPROM region
            return self.read_eeprom(address, length);
        }
        self.flash
            .read(address, &mut self.buffer[0..length])
            .map_err(|_| DFUMemError::Unknown)?;

        Ok(&self.buffer[0..length])
    }

    fn erase(&mut self, address: u32) -> Result<(), DFUMemError> {
        // Only erase when address matches 4k sector boundary
        if address & 0x0000_0fff == 0x0 {
            self.flash.erase_sectors(address, 1).ok();
        }
        // hprintln!("E:{}", address);
        Ok(())
    }

    fn erase_all(&mut self) -> Result<(), DFUMemError> {
        self.flash.erase_all().ok();
        Ok(())
    }

    fn store_write_buffer(&mut self, src: &[u8]) -> Result<(), ()> {
        self.buffer[..src.len()].copy_from_slice(src);
        // If you enable this for debugging, increase PROGRAM_TIME
        // hprintln!("S:{}", src.len());
        Ok(())
    }

    fn program(&mut self, address: u32, length: usize) -> Result<(), DFUMemError> {
        // TODO: check address value
        if length > Self::TRANSFER_SIZE.into() {
            return Err(DFUMemError::Prog);
        }
        // If you enable this for debugging, increase PROGRAM_TIME
        // hprintln!("P:{} {}", address, length);
        if address == CONTROL_AREA_ADDRESS{
            let new_da = self.buffer[..4].try_into().unwrap();
            self.read_update_display_address(u32::from_le_bytes(new_da));
            return Ok(());
        }
        self.flash
            .write_bytes(address, &mut self.buffer[0..length])
            .ok();

        // TODO: verify that memory is programmed correctly
        Ok(())
    }

    fn manifestation(&mut self) -> Result<(), DFUManifestationError> {
        // Nothing to do to activate FW
        Ok(())
    }
}

impl<'a> SpiFlash<'a> {
    // This is the size of a Lightnote page.  It corresponds to one
    // display-worth of data (5000B) rounded up to the closest erase sector
    // boundary (4094 * 2)
    // const LIGHTNOTE_PAGE_SIZE: u32 = 0x2_000; // 8192

    pub(crate) fn new(
        spi_flash: SpiFlashMainType<'a>,
        mut cs_flash: PB6<Output<PushPull>>,
        delay: &mut Delay,
    ) -> Self {
        // Wiggle chip select seems to avoid Flash::init failures that occur in
        // transient power losses in the middle of a memory read
        cs_flash.set_high().unwrap();
        delay.delay_ms(100u32);
        cs_flash.set_low().unwrap();

        let flash = Flash::init(spi_flash, cs_flash);
        // if flash.is_err() {
        //     errors::raise(
        //         LightNoteErrors::FailedToInitializeFlash,
        //         &mut led_grn,
        //         &mut led_ylw,
        //         &mut delay,
        //     );
        //     SCB::sys_reset();
        // }
        let flash = flash.unwrap();
        SpiFlash {
            flash,
            buffer: [0u8; 1024],
        }
    }

    // pub(crate) fn sleep(self: &mut Self) {
    //     self.flash.sleep().unwrap();
    // }

    pub(crate) fn check_flash_id(self: &mut Self) -> Result<(), LightNoteErrors> {
        for _ in 0..20 {
            if let Ok(id) = self.flash.read_jedec_id() {
                if id.device_id() == [0x40, 0x18] {
                    return Ok(());
                }
            }
        }
        return Err(LightNoteErrors::FailedToReadFlashID);
    }

    fn read_eeprom(&mut self, address: u32, length: usize) -> Result<&[u8], DFUMemError> {
        let p = unsafe { pac::Peripherals::steal() };
        let mut rcc = p.RCC.freeze(Config::hsi16());
        let nvm = Nvm::new(p.FLASH, &mut rcc);
        nvm.read_raw(
            &mut self.buffer[0..length],
            address - EEPROM_VIRTUAL_ADDRESS,
            length,
        )?;

        Ok(&self.buffer[0..length])
    }

    fn read_update_display_address(&mut self, new_address: u32) {
        let p = unsafe { pac::Peripherals::steal() };
        let mut rcc = p.RCC.freeze(Config::hsi16());
        let mut nvm = Nvm::new(p.FLASH, &mut rcc);
        // hprintln!("DA: {:08x}", new_address).ok();
        nvm.save_display_addr(new_address);
        nvm.save_answer_pending(false);
    }

    // pub(crate) fn release(self: &mut Self) -> (SpiFlashMainType, PB6<Output<PushPull>>) {
    //     self.flash.release()
    // }
}

type SpiFlashMainType<'a> = SpiProxy<
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

type SpiFlashWithCsType<'a> =
    Flash<SpiFlashMainType<'a>, PB6<stm32l0xx_hal::gpio::Output<stm32l0xx_hal::gpio::PushPull>>>;

pub(crate) struct SpiFlash<'a> {
    flash: SpiFlashWithCsType<'a>,
    buffer: [u8; 1024],
}