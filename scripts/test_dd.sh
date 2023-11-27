COUNT=8
BLOCK_SIZE=512
sudo dd if=/dev/urandom of=/tmp/tin.img count=${COUNT} bs=${BLOCK_SIZE}
sudo sg_dd blk_sgio=1 if=/tmp/tin.img of=/dev/sde count=${COUNT} bs=${BLOCK_SIZE}
sudo sg_dd blk_sgio=1 if=/dev/sde of=/tmp/tout.img count=${COUNT} bs=${BLOCK_SIZE}
diff /tmp/tin.img /tmp/tout.img || { echo FAIL; exit 1; }
echo PASS
