# Zune-JPEG

A pretty fast multithreaded JPEG decoder with x86_64 SIMD accelerated functions

# Features

- [x] A Pretty fast 8*8 integer IDCT
- [x] Fast Huffman Decoding
- [x] Fast color convert functions for AVX2 and SSE have been implemented
- [x] Support for extended colorspaces like RGBX and RGBA
- [X] Multi-threaded decoding 

# Benchmarks

See [Benches.md](/Benches.md)

# Features  

|feature|on|Capabilities|
|-------|---|------------|
|`x86`| ✅ | Enables `x86` specific instructions, specifically `avx` and `sse` for accelerated decoding.|
| `threadpool`| ✅ |Enables multithreading decoding of images, about 30% decoding improvement when enabled|



# TODO

- [ ] Add up-sampling algorithms
- [ ] Add support for interleaved images
- [ ] Add support for progressive JPEGS

Good breakdown of JPEG [here](https://github.com/corkami/formats/blob/master/image/jpeg.md)
