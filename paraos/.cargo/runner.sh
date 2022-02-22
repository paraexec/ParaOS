#! /usr/bin/env bash

kernel=$1
target_dir=$(dirname "$kernel")

mkdir -p "${target_dir}"/disk/bootboot
cp -f support/bootboot.efi "${target_dir}"/disk
echo "BOOTBOOT.EFI" > "${target_dir}"/disk/startup.nsh
cp "${kernel}" "${target_dir}"/disk/bootboot/x86_64

qemu-system-x86_64 \
    -drive if=pflash,format=raw,readonly=on,file=support/OVMF.fd \
    -drive if=pflash,format=raw,readonly=off,file=support/OVMF_VARS.fd \
    -drive format=raw,file=fat:rw:"${target_dir}"/disk \
    -cpu max -serial mon:stdio -machine q35 -smp 4 -s \
    -nographic -device isa-debug-exit
error_code=$?
# shellcheck disable=SC2004
exit $(($error_code >> 1))

