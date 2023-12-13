use core::cell::RefCell;

// use core::sync::atomic::AtomicU32;
use cortex_m::prelude::_embedded_hal_blocking_delay_DelayMs;
// use cortex_m_semihosting::hprintln;
use shared_bus::{NullMutex, SpiProxy};
use w25q::{
    series25::Flash,
};
use stm32l0xx_hal::{
    delay::Delay,
    gpio::{
        gpiob::{PB3, PB4, PB5, PB6},
        Output, PushPull,
    },
    prelude::OutputPin,
};

use usbd_scsi::{BlockDevice, BlockDeviceError};

use crate::{errors::LightNoteErrors, nvm};

impl From<nvm::Error> for BlockDeviceError {
    fn from(value: nvm::Error) -> Self {
        match value {
            nvm::Error::InvalidAddress => BlockDeviceError::InvalidAddress,
        }
    }
}

const FLASH_SECTOR_SIZE: usize = 4096;

impl BlockDevice for SpiFlash<'_> {
    const BLOCK_BYTES: usize = 1024;

    fn read_block(&mut self, lba: u32, block: &mut [u8]) -> Result<(), BlockDeviceError> {
        // defmt::info!("read_block {}", lba);
        self.flash
            .borrow_mut()
            .read(lba * Self::BLOCK_BYTES as u32, block)
            .map_err(|_| BlockDeviceError::HardwareError)?;
        Ok(())
    }

    fn write_block(&mut self, lba: u32, block: &[u8]) -> Result<(), BlockDeviceError> {
        if self.is_block_erased(lba)? {
            self.write_block_fast(lba, block)
        } else {
            self.write_block_slow(lba, block)
        }
    }

    fn erase_device(&mut self) -> Result<(), BlockDeviceError> {
        self.flash.get_mut().erase_all().map_err(|_| BlockDeviceError::EraseError)?;
        Ok(())
    }

    fn max_lba(&self) -> u32 {
        16 * 1024 * 1024 / Self::BLOCK_BYTES as u32 - 1
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
            flash: RefCell::new(flash),
        }
    }

    fn is_block_erased(&mut self, lba: u32) -> Result<bool, BlockDeviceError> {
        // read
        let mut buffer = [0u8; Self::BLOCK_BYTES];
        self.flash
            .get_mut()
            .read(lba * Self::BLOCK_BYTES as u32, &mut buffer)
            .map_err(|_| BlockDeviceError::HardwareError)?;
        for v in buffer.iter() {
            if *v != 0xff {
                return Ok(false)
            }
        }
        Ok(true)
    }


    // Assumes chip has been erased
    fn write_block_fast(&mut self, lba: u32, block: &[u8]) -> Result<(), BlockDeviceError> {
        defmt::info!("write_block_fast {}, block size: {}", lba, block.len());

        // write
        self.flash
            .get_mut()
            .write_bytes(lba * Self::BLOCK_BYTES as u32, block)
            .ok();
        Ok(())
    }

    fn write_block_slow(&mut self, lba: u32, block: &[u8]) -> Result<(), BlockDeviceError> {
        // Each flash sector contains 8 512 blocks.  Find the offset into the sector
        // that needs to be modified
        let blocks_per_sector = FLASH_SECTOR_SIZE / Self::BLOCK_BYTES;
        let offset = (lba % blocks_per_sector as u32) as usize;
        let sector_start = (lba as usize - offset) * Self::BLOCK_BYTES;
        defmt::info!(
            "write_block_slow {} offset 0x{:x} into sector starting at 0x{:x}",
            lba,
            offset,
            sector_start
        );

        // read
        let mut buffer = [0u8; FLASH_SECTOR_SIZE];
        self.flash
            .get_mut()
            .read(sector_start as u32, &mut buffer)
            .map_err(|_| BlockDeviceError::HardwareError)?;

        // erase
        self.flash
            .get_mut()
            .erase_sectors(sector_start as u32, 1)
            .map_err(|_| BlockDeviceError::EraseError)?;

        // modify
        buffer[(offset * Self::BLOCK_BYTES)..((offset + 1) * Self::BLOCK_BYTES)]
            .copy_from_slice(block);

        // write
        self.flash
            .get_mut()
            .write_bytes(sector_start as u32, &mut buffer)
            .ok();

        // verify
        self.flash
            .get_mut()
            .read(sector_start as u32, &mut buffer)
            .map_err(|_| BlockDeviceError::HardwareError)?;

        if &buffer[(offset * Self::BLOCK_BYTES)..((offset + 1) * Self::BLOCK_BYTES)] != block {
            defmt::error!(
                "Verify failed for block 0x{:x}..0x{:x}",
                offset * Self::BLOCK_BYTES,
                (offset + 1) * Self::BLOCK_BYTES
            );
        }
        Ok(())
    }


    // pub(crate) fn sleep(self: &mut Self) {
    //     self.flash.sleep().unwrap();
    // }

    #[allow(dead_code)]
    pub(crate) fn check_flash_id(self: &mut Self) -> Result<(), LightNoteErrors> {
        for _ in 0..20 {
            if let Ok(id) = self.flash.get_mut().read_jedec_id() {
                if id.device_id() == [0x40, 0x18] {
                    return Ok(());
                }
            }
        }
        return Err(LightNoteErrors::FailedToReadFlashID);
    }
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

pub struct SpiFlash<'a> {
    flash: RefCell<SpiFlashWithCsType<'a>>,
}
