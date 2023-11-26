#! /bin/bash
#

#set -x
[ -z "$1" ] && { echo usage: $0 "<filename>"; exit 1; }
FILE=$1

FILE_SIZE=$(stat -c%s ${FILE})
LBA_SIZE=512
FILE_SIZE_ALIGNED=$(( ((FILE_SIZE + (LBA_SIZE - 1)) / LBA_SIZE) * LBA_SIZE ))
FILE_SIZE_ALIGNED_LBA=$(( FILE_SIZE_ALIGNED / LBA_SIZE ))

FLASH_SIZE_K=$(( (FILE_SIZE_ALIGNED_LBA + 1023) / 1024))
FS_OVERHEAD_K=7
dd if=/dev/zero of=src/disk.img bs=1K count=$((FLASH_SIZE_K + FS_OVERHEAD_K))
mformat -i src/disk.img ::
mcopy -i src/disk.img ${FILE} ::


START_FILE=$(xxd ${FILE} | head -1 | cut -d: -f2)
START_FILE=${START_FILE:0:40}
# Offset is aligned to LBA (512 bytes or 0x200).  Use that as the block size for dd for faster execution
OFFSET=$(xxd src/disk.img | grep "${START_FILE}" | cut -d: -f1)
OFFSET_LBA=$(( 0x${OFFSET} / ${LBA_SIZE} ))

mdir -i src/disk.img ::
echo ---------------File details-------------------
echo Size LBA-aligned:
echo \ \ \ bytes: ${FILE_SIZE_ALIGNED} \( ${FILE_SIZE_ALIGNED_LBA} LBAs \)
echo Start: 
echo \ \ \ LBA: ${OFFSET_LBA} \( $(printf 0x%x ${OFFSET_LBA}) \)
echo \ \ \ address: $(( OFFSET_LBA * LBA_SIZE )) \( $(printf 0x%x $(( OFFSET_LBA * LBA_SIZE )) ) \)
echo End: 
echo \ \ \ LBA: $(( OFFSET_LBA + FILE_SIZE_ALIGNED_LBA )) \( $(printf 0x%x $(( OFFSET_LBA + FILE_SIZE_ALIGNED_LBA )) ) \)
echo \ \ \ address: $(( LBA_SIZE * (OFFSET_LBA + FILE_SIZE_ALIGNED_LBA) )) \( $(printf 0x%x $(( LBA_SIZE * (OFFSET_LBA + FILE_SIZE_ALIGNED_LBA) )) ) \)
