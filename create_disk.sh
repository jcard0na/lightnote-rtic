#! /bin/bash
#

# filename adheres to 8.3 format
echo 'Hello world' > lghtnote.txt

dd if=/dev/zero of=src/disk.img bs=1K count=8
mformat -i src/disk.img ::
mcopy -i src/disk.img lghtnote.txt ::
mdir -i src/disk.img ::

rm -f lghtnote.txt
