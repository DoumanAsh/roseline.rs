#!/bin/bash

if [ $TARGET = x86_64-unknown-linux-gnu ]; then
    cargo --version
else
    cargo install cross || true
    cross --version
fi
