# Benchmarks of popular jpeg libraries


Here I compare how long it takes popular JPEG decoders to decode the below 7680*4320 image
of [Cutefish OS](https://en.cutefishos.com/) default wallpaper.
![img](test-images/speed_bench.jpg)
Currently due to this library's limitation, I can't add other sub-sampled and progressive images benchmarks, but
I will add them when I implement them.

The benchmarks are carried out on my local machine with an AMD Ryzen 5 4500u

The benchmarks are reproducible. 

To reproduce them
1. Clone this repository
2. Install rust(if you don't have it yet)
3. `cd` into the directory. 
4. Run `cargo bench`
### RGB

### 1 * 1(No upsampling) Baseline RGB Decoding

|Decoder | Link | Speed|
|--------|-------|-----|
|**zune-jpeg**|- | 42.246 ms |
|libjpeg-turbo| [github-link]|68.343 ms|
|image-rs| [link] |81.350 ms |


Yaay almost twice as fast as `image-rs/decoders` and 50% faster than `libjpeg-turbo`

# GrayScale
### 1*1 Baseline grayscale decoding.

|Decoder | Link | Speed|
|--------|-------|-----|
|**zune-jpeg**|- | 37.598 ms |
|libjpeg-turbo| [github-link]|41.648 ms|
 
Not much improvement on my side, but libjpeg-turbo made some quantum leaps 50% improvement...
 Image-rs/jpeg-decoder does not support grayscale decoding, hence it wasn't included in the benchmark

[github-link]:https://github.com/libjpeg-turbo/libjpeg-turbo
[link]:https://github.com/image-rs/jpeg-decoder