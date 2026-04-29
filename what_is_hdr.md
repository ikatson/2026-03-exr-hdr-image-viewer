What is HDR, really

Target audience:
- Tech savvy users, software engineers

# RGB - colors as numbers
RGB: red, green, blue. Pixels in colorful images can, and often are represented using 3 numbers.
What those numbers mean, i.e. what real-world "color" and "brightness" they represent is much more nuanced. The rest of the article will cover that.
Red, green and blue are called "channels".

A notable exception is movies: they are usually stored as YUV - also 3 numbers, that mathematically can be converted between to and from RGB.
For simplicity, just assume you can always convert YUV to RGB and back, and let's ignore it for now.

# Color spaces
A color space is a mathematical model of how numbers map to a real-world color. What is a real world color you may ask? Some mix of wavelengths - i.e. something that has physical meaning.
There's a reference color space "CIE XYZ", where numbers map to real-world colors.

All other color spaces are usually defined in terms of its relation to XYZ.

# Linear color
The CIE XYZ colorspace is linear - if the numbers are scaled by a constant, e.g. multiplied by 2, it means the color will be the same, but the luminance (physical brightness) will be 2x higher.

# Encoding / quantization / nonlinearity / transfer function / gamma
The RGB triplets are conceptually real numbers, which have infinitely many values (e.g. there's an infinite number of values between 0 and 1).
In the digital world we can't fit infinite precision into a finite number of bytes / bits.

So we need to "quantize" - compress the range into the available bandwidth.
Imagine a color space, and one of it's channels let's call it "red", where 0 would mean absence of red and 1 would mean maximum red.

If we had 1 byte to store this range (as is typical for most images), we can only store 256 possible values.
Naively, we could linearly slice the input range into 256 values 
- 0x01 = 1/256 ~ 0.0039
- 0x02 = 2/256 ~ 0.0078
- ..
- 0x09 = 9/256 ~ 0.0351
- ...
- 0xfe = 255/256 ~ 0.996
- 0xff = 1

Unfortunately, this isn't great in practice - the main reason being that humans perceive differences in dark values much better than in bright values.
So we need to do smth else to allocate more bits for the dark areas.

This is done with a math function called "transfer function". They are different per color space.
Before this function is applied the numbers are called "optical" or "linear", corresponding to the real world. Afterwards "electronic".
The function can be inversed, e.g. by your monitor / TV - when it reads an RGB triplet it needs to map that to emitted light.

OETF - opto-electronic transfer function - to encode a linear ("optical") color for digital representation
EOTF - eltro-optical transfer function - to convert digital representation into "how much light should I emit". Simplifying a bit you can think of it as the inverse of OETF, although it's a bit more nuanced in practice.

A common transfer function is called "gamma" - which is just some power of the input (assumes the input is between 0 and 1). 

For example, for an input channel value=0.5 if you apply gamma=2.2 OETF it'll become pow(0.5, 1/2.2)=~0.73.
To invert (apply EOTF) you do do pow(x, 2.2).

Then to store than in a byte, you do 0.73 * 256 = 186 = 0xba

Here's how the above quantization would change with a gamma transfer function:
- 0x01 ~= 0.0000050335
- 0x02 ~= 0.0000231280
- ...
- 0x09 ~= 0.0006327113
- ...
- 0xba ~= 0.495 (our example above)
- ...
- 0xf3 ~= 0.991
- 0xff = 1.

Notice how we can represent really low numbers this way! This way our eyes won't be able to perceive quantization artifacts.

# SDR
To understand HDR, we need to understand SDR, as it's easier to compare them than to explain HDR in vacuum.


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
