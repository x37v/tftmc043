#!/bin/bash
udisksctl mount -b /dev/disk/by-label/RPI-RP2
cargo run
