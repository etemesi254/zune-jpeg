# Zune-JPEG
A pretty fast JPEG decoder with x86_64 SIMD accelerated functions


# Features
- [x] A Pretty fast 8*8 integer IDCT
- [x] Fast Huffman Decoding
- [x] Fast color convert functions for AVX2 and SSE have been implemented
- [x] Support for extended colorspaces like RGBX and RGBA
- [X] Multi-threaded decoding

# Benchmarks
The benchmarks compare this library and the time [libjpeg_turbo](https://github.com/libjpeg-turbo/libjpeg-turbo) , and [image_rs/jpeg decoder](https://github.com/image-rs/jpeg-decoder)
takes to decode this [4,476x2,984 (13.4MP) image](https://raw.githubusercontent.com/elementary/wallpapers/master/backgrounds/Mr.%20Lee.jpg)
```text
Baseline JPEG Decoding zune-jpeg                                                                            
                        time:   [49.826 ms 49.935 ms 50.042 ms]
                        
Baseline JPEG Decoding  mozjpeg                                                                            
                        time:   [67.240 ms 67.262 ms 67.284 ms]
                        
Baseline JPEG Decoding  imagers/jpeg-decoder                                                                            
                        time:   [67.125 ms 67.187 ms 67.270 ms]

```
# TODO
- [ ] Add up-sampling algorithms
- [ ] Add support for interleaved images
- [ ] Add support for progressive JPEGS

Good breakdown of JPEG [here](https://github.com/corkami/formats/blob/master/image/jpeg.md)