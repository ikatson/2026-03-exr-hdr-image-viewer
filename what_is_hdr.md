What is HDR, really

Target audience:
- Tech savvy users, software engineers

# Basic terminology

## RGB
RGB: red, green, blue. Pixels in colorful images can, and often are represented using 3 numbers.
What those numbers mean, i.e. what real-world "color" and "brightness" they represent is more nuanced. The rest of the article will cover that.
Red, green and blue are called "channels".

A notable exception is movies: they are usually stored as YUV - also 3 numbers, and notably, interchangable with RGB.
For simplicity, just assume you can always convert YUV to RGB and back, and let's ignore it for now.

## 8 bit color
8 bits = 1 byte - covers 256 values (2**8). A common encoding for RGB triplets.

## 10 bit color
Can store 1024 values per channel. (2**10)

To understand HDR, let's explain SDR first.

# SDR
Have you ever seen "RGB" a color represented as 3 values from 0 to 255? That's SDR.

The color and brightness in the majority of digital images and online content is encoded this way.
0-255 are integer values that you can store in 1 byte (8 bits).
So to store an uncompressed RGB color of one pixel you need 3 bytes.

E.g. (R=255, G=0, B=0) is "red" color at "max brightness" (more on that below).

## What is "red" in the example above?
To properly explain it we'll dive into Color Spaces below.
But in general, that means a specific "red" - the one from rec709 / sRGB color space
Usually, sRGB color space is assumed
But in a nutshell, in the majority of cases that'll mean a specific "red" 

## What is "max brightness" in the example above?

That's where the casual knowledge usually end, and it's enough for a ton of applications.
For example, when creating a web page, you pick colors from 
What is "red" actually?
Think .jpg or .png.
Usually
