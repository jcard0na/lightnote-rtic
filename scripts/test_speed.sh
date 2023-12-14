#! /bin/bash
#
#COUNT=32768
COUNT=32
BLOCK_SIZE=4096
SKIP_COUNT=1000
[ -z "$1" ] && { echo "usage: $0 </dev/sdX>"; exit 1; }
DEVICE=$1
# set -x

echo Erase entire flash...
sudo sg_write_same --10 --ff --num 0 --lba 0 --xferlen 1 ${DEVICE}

echo Test operation speed of ${COUNT} blocks \( $(( BLOCK_SIZE * COUNT )) bytes: \)

echo Clean write:
sudo sg_dd bpt=30 blk_sgio=1 if=/dev/random of=${DEVICE} count=${COUNT} bs=${BLOCK_SIZE} seek=${SKIP_COUNT} verbose=10 --progress |& grep '/sec' | sed -e 's/.*at//'

echo Erase+write:
sudo sg_dd bpt=30 blk_sgio=1 if=/dev/random of=${DEVICE} count=${COUNT} bs=${BLOCK_SIZE} seek=${SKIP_COUNT} verbose=10 --progress |& grep '/sec' | sed -e 's/.*at//'

echo Read:
sudo sg_dd bpt=30 blk_sgio=1 if=${DEVICE} of=/dev/null count=${COUNT} bs=${BLOCK_SIZE} skip=${SKIP_COUNT} verbose=10 --progress |& grep '/sec' | sed -e 's/.*at//'

echo DONE
