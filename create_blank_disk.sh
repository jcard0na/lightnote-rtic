#! /bin/bash
#

#set -x
pushd src
FLASH_SIZE=$((16 * 1024 * 1024 ))
FLASH_SIZE_K=$(( FLASH_SIZE / 1024))
dd if=/dev/zero of=disk.img bs=1K count=$((FLASH_SIZE_K))
mformat -i disk.img -v lightnote ::

# Show non-empty areas
xxd disk.img | grep -v '0000 0000 0000 0000 0000 0000 0000 0000'

BOOT_SECTOR_OFFSET=0x00000000
BOOT_SECTOR_SIZE=0x70
dd if=disk.img of=${BOOT_SECTOR_OFFSET}.bin bs=16  count=$(( BOOT_SECTOR_SIZE / 16 )) skip=$(( BOOT_SECTOR_OFFSET )) &> /dev/null
echo ===== BOOT SECTOR \( $( printf '0x%x' ${BOOT_SECTOR_OFFSET}) \) =====
xxd ${BOOT_SECTOR_OFFSET}.bin

FAT_REGION_OFFSET=0x000001b0
FAT_REGION_SIZE=0x60
echo ===== FAT REGION 1\( $( printf '0x%x' ${FAT_REGION_OFFSET}) \) =====
dd if=disk.img of=${FAT_REGION_OFFSET}.bin bs=16  count=$(( FAT_REGION_SIZE / 16 )) skip=$(( FAT_REGION_OFFSET / 16)) &> /dev/null
xxd -o ${FAT_REGION_OFFSET} ${FAT_REGION_OFFSET}.bin

FAT_REGION_OFFSET=0x00010000
FAT_REGION_SIZE=0x10
echo ===== FAT REGION 2\( $( printf '0x%x' ${FAT_REGION_OFFSET}) \) =====
dd if=disk.img of=${FAT_REGION_OFFSET}.bin bs=16  count=$(( FAT_REGION_SIZE / 16 )) skip=$(( FAT_REGION_OFFSET / 16)) &> /dev/null
xxd -o ${FAT_REGION_OFFSET} ${FAT_REGION_OFFSET}.bin

ROOT_DIRECTORY_OFFSET=0x0001fe00
ROOT_DIRECTORY_SIZE=0x20
dd if=disk.img of=${ROOT_DIRECTORY_OFFSET}.bin bs=16  count=$(( ROOT_DIRECTORY_SIZE / 16 )) skip=$(( ROOT_DIRECTORY_OFFSET / 16)) &> /dev/null
echo ===== ROOT DIRECTORY \( $( printf '0x%x' ${ROOT_DIRECTORY_OFFSET}) \) =====
xxd -o ${ROOT_DIRECTORY_OFFSET} ${ROOT_DIRECTORY_OFFSET}.bin

popd
