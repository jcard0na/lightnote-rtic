#! /bin/bash
#

set -x

dd if=/dev/urandom of=lightnote.rom bs=1M count=16
FLASH_SIZE_K=$((16777216 / 1024))
FS_OVERHEAD_K=256
dd if=/dev/zero of=src/disk.img bs=1K count=$((FLASH_SIZE_K + FS_OVERHEAD_K))
mformat -i src/disk.img ::
mcopy -i src/disk.img lightnote.rom ::
mdir -i src/disk.img ::

START_FILE=$(xxd lightnote.rom | head -1 | cut -d: -f2)
START_FILE=${START_FILE:0:40}
# Offset is aligned to LBA (512 bytes or 0x200).  Use that as the block size for dd for faster execution
LBA_SIZE=512
OFFSET=$(xxd src/disk.img | grep "${START_FILE}" | cut -d: -f1)
OFFSET_LBA=$(( 0x${OFFSET} / ${LBA_SIZE} ))
dd if=src/disk.img of=test.rom bs=${LBA_SIZE} skip=${OFFSET_LBA} count=$(( FLASH_SIZE_K * 2 ))
diff test.rom lightnote.rom && { echo PASS: File is contiguous; exit 0; }
echo FAIL
