#!/bin/bash


EFI_FILE="target/x86_64-unknown-uefi/debug/ratmir_oc.efi"


mkdir -p esp/EFI/BOOT
cp $EFI_FILE esp/EFI/BOOT/BOOTX64.EFI

qemu-system-x86_64 \
    -m 2G \
    -drive if=pflash,format=raw,readonly=on,file=/usr/share/edk2/ovmf/OVMF_CODE.fd \
    -drive format=raw,file=fat:rw:esp \
    -net none