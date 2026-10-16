# AsciiFlow application icon

`asciiflow.svg` is the editable source. The AF monogram consists of vector paths,
so exporting it does not depend on installed fonts. The dark square and mint
monogram are original artwork, licensed under the repository's MIT license.

`asciiflow.png` is the 512 × 512 RGBA window/launcher image.
`asciiflow.ico` contains 256, 128, 64, 48, 32 and 16 pixel images for Windows
shortcuts. Both release archives include this directory. Windows executable
resource embedding is not configured.

Regenerate the raster assets from the repository root with ImageMagick 7:

```sh
magick -background none assets/asciiflow.svg -depth 8 PNG32:assets/asciiflow.png
magick assets/asciiflow.png -define icon:auto-resize=256,128,64,48,32,16 assets/asciiflow.ico
```
