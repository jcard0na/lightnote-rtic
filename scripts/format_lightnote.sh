#! /bin/bash
#

#set -x
[ -z "$1" ] && { echo "usage: $0 </dev/sdX>"; exit 1; }
DEVICE=$1
FLASH_SIZE=$((16 * 1024 * 1024 ))
LBA_SIZE=512
FLASH_SIZE_LBA=$(( FLASH_SIZE / LBA_SIZE))
dd if=/dev/zero of=disk.img bs=${LBA_SIZE} count=$((FLASH_SIZE_LBA)) &> /dev/null
mformat -i disk.img -v lightnote ::

# Write all 0xff to erase entire flash
sudo sg_write_same --10 --ff --num 0 --lba 0 --xferlen 1 ${DEVICE}

# Actual write
sudo sg_dd blk_sgio=1 if=disk.img of=${DEVICE} bs=${LBA_SIZE} count=$((FLASH_SIZE_LBA)) --progress --progress --progress
