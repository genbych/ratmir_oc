#!/bin/bash

USB_PART="/dev/sda1"
MNT_POINT="/mnt/ratmir_usb"

echo "--- Сборка проекта ---"
cargo build --target x86_64-unknown-uefi

echo "--- Подготовка фleшки ---"
sudo mkdir -p $MNT_POINT
sudo mount $USB_PART $MNT_POINT


sudo mkdir -p $MNT_POINT/EFI/BOOT

echo "--- Копирование бинарника ---"
sudo cp target/x86_64-unknown-uefi/debug/ratmir_oc.efi $MNT_POINT/EFI/BOOT/BOOTX64.EFI


sync
sudo umount $MNT_POINT
echo "--- Готово! Можно вынимать флешку ---"