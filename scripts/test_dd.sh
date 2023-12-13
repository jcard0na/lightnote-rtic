#! /bin/bash
#
#COUNT=32768
COUNT=8
BLOCK_SIZE=1024
SKIP_COUNT=8
[ -z "$1" ] && { echo "usage: $0 </dev/sdX>"; exit 1; }
DEVICE=$1
I=/tmp/tin.img
O1=/tmp/tout1.img
O2=/tmp/tout2.img
sudo rm -f $I $O1 $O2
sudo dd if=/dev/urandom of=$I count=${COUNT} bs=${BLOCK_SIZE}
sudo sg_dd blk_sgio=1 if=$I of=${DEVICE} count=${COUNT} bs=${BLOCK_SIZE} seek=${SKIP_COUNT} --progress
sudo sg_dd blk_sgio=1 if=${DEVICE} of=$O1 count=${COUNT} bs=${BLOCK_SIZE} skip=${SKIP_COUNT} --progress
sudo sg_dd blk_sgio=1 if=${DEVICE} of=$O2 count=${COUNT} bs=${BLOCK_SIZE} skip=${SKIP_COUNT} --progress
diff $I $O1 || { ./bindiff.sh $I $O2; echo FAIL; exit 1; }
diff $I $O2 || { ./bindiff.sh $O1 $O2; echo FAIL; exit 1; }
echo PASS
