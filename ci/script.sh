#!/bin/bash

if [ $TARGET = x86_64-unknown-linux-gnu ]; then
    cargo test
else
    cross test --target $TARGET
fi
