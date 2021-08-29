# Zune-JPEG
A pretty amazing JPEG decoder with x86_64 acceleration functions

# Warning:Currently works on CPUs that support AVX2 instructions

# Things already done
- [x] A Pretty fast 8*8 IDCT has been implemented, and is currently being used
for decoding
- [x] Fast color convert functions for AVX2 and SSE have been implemented
- [x] Support for extended colorspaces like RGBX and RGBA
# TODO
- [ ] Add up-sampling algorithms
- [ ] Add support for interleaved images
- [ ] Add support for progressive JPEGS
- [ ] Add Generic Color convert functions

# Some amazing resources

Some stuff about floating point conversions: (And why they are expensive):[here](http://justinparrtech.com/JustinParr-Tech/programming-tip-turn-floating-point-operations-in-to-integer-operations/)

Good breakdown of JPEG [here](https://github.com/corkami/formats/blob/master/image/jpeg.md)