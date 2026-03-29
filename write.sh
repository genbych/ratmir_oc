#!/bin/bash

sudo dd if=/home/user/dev/Rust/ratmir_oc/target/x86_64-ratmir_oc/debug/bootimage-ratmir_oc.bin of=/dev/sda && sync