#!/bin/bash

if [ $TARGET = x86_64-unknown-linux-gnu ]; then
    cargo build --package roseline_launch --release
    cargo build --package roseline --release
    cargo build --package roseline-web --release
    zip "$PROJECT_NAME-$TRAVIS_TAG-$TARGET.zip" -j target/release/roseline roseline.toml target/release/roseline-web target/release/roseline_launch
else
    cross build --package roseline_launch --release --target $TARGET
    travis_wait 40 sleep infinity & cross build --package roseline --release --target $TARGET
    cross build --package roseline-web --release --target $TARGET
    zip "$PROJECT_NAME-$TRAVIS_TAG-$TARGET.zip" -j target/$TARGET/release/roseline roseline.toml target/$TARGET/release/roseline-web target/$TARGET/release/roseline_launch
fi
