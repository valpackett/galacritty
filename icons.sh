#!/usr/bin/env zsh
app="technology.unrelenting.galacritty"
for i in 16 24 32 48 64 128 256; do
    rsvg-convert -w $i -h $i res/icons/hicolor/scalable/apps/$app.svg \
        | pngquant --quality=70-90 - > res/icons/hicolor/${i}x${i}/apps/$app.png
done
