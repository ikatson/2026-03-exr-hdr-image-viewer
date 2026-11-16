What is HDR, really

Target audience:
- Tech savvy users, software engineers

# RGB - colors as numbers
RGB: red, green, blue. Pixels in colorful images can, and often are represented using 3 numbers.
What those numbers mean, i.e. what real-world "color" and "brightness" they represent is much more nuanced. The rest of the article will cover that.
Red, green and blue are called "channels".

A notable exception is movies: they are usually stored as YUV - also 3 numbers, where Y is the "luminance" - how bright a pixel should be. You can always losslessly convert (in theory) between YUV and RGB as it's a simple matrix transformation. This is done, so that U and V (color information) can be compressed, but also for historical reasons as TV started with just "Y" and "U and V" were added later to make color TV.

For simplicity, just assume you can always convert YUV to RGB and back, and let's ignore it for now.

# Color spaces
A color space is a mathematical model of how numbers map to a real-world color. What is a real world color you may ask? Some mix of wavelengths - i.e. something that has physical meaning.
There's a reference color space "CIE XYZ", where numbers map to real-world colors.

All other color spaces are usually defined in terms of its relation to XYZ.

Color spaces are usually defined as:
- red, green, blue primaries expressed as CIE XYZ numbers. I.e. what is "red", "green" and "blue" in real-world values.
- white point. What is "white" - as a mix of red, green and blue. You may have seem "warm" or "cold" setting on your display for white balance - this shows that "white" is as relative as all other color names unless grounded in physical values which XYZ does.
- transfer function - more on this below

# Linear color
The CIE XYZ colorspace is linear - if the numbers are scaled by a constant, e.g. multiplied by 2, it means the color will be the same, but the luminance (physical brightness) will be 2x higher.

# Encoding / quantization / nonlinearity / transfer function / gamma
The RGB triplets are conceptually real numbers, which have infinitely many possible values (e.g. there's an infinite number of values between 0 and 1).
In the digital world we can't fit infinite precision into a finite number of bytes / bits.

So we need to "quantize" - compress the range into the available bandwidth.
Imagine a color space, and one of its channels, let's call it "red", where 0 would mean absence of red and 1 would mean maximum red.

#### TODO HERE AND BELOW - need to use 255 in math examples

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

SDR means:
- sRGB (images, games) or rec709 (movies, tv) color space
  - they have the same primaries and white point
  - different transfer function (approximately gamma 2.2 and gamma 2.4 respectively)
- 8 bits per channel quantization
- luminance is relative to your display's max brightness. E.g. 1.0 = max brightness of your display.
  - It follows that values over 1.0 are out of range - as your display can't become brighter than its own max brightness

Let's take an example random image on the web.
If its file metadata doesn't explain what color space the values are in, they are in sRGB, by convention. If there was no color space (even implicit), there would be no meaning at all to the pixel values stored in the file!

Let's say this image has a pixel with RGB (0xff, 0xba, 0xba) or (255, 186, 186).
This maps to a well defined real-world color (in CIE XYZ). Example for green channel:
- normalize to 0-1: 186/255 ~= 0.73
- apply EOTF: (186 / 255)**2.2 ~= 0.5 - this makes the color linear. 
  - If you were making a display, you could send half of max voltage to the green subpixel (if it was perfectly calibrated for sRGB colors and had a perfect voltage-to-light ratio)

# HDR
Finally we have everything we need to understand the basics of what is HDR.

TODO

To cover: color space bt 2020 - MINOR kind of. More range - for luminance mostly.
HDR capture, HDR display, HDR representations (PQ is what is "on the wire", but it's quantized).
The problem with absolute values - doesn't make sense in a bright room, at all! So HDR is kind of stupid for being absolute (in movies).
In games, you could usually tweak the formula for transfer function (describe how a game image rendered using float absolute values gets converted to PQ in shaders using tone mapping). But devs don't know how to do it well so hence RenoDX / Luma etc.
Make it all visual - but referenced in numbers. How the same exact thing can be displayed in mutiple manners. Include some AVIF (or whatever is supported) pictures to compare.

Another thing: someone said on Hacker News: "Except that "some level" in even my previous Nikon D800 camera has been a 14-bit RAW with 13.5 stops of dynamic range, which I got back in 2012! That puts it somewhere between the VESA DisplayHDR 600 and 1000 standards, which is plenty HDR." - so many people are confused about what HDR is and isn't, and this comparison is stupid - comparing contrast ratio to HDR peak values is nonsense. Cameras capture MUCH MUCH more peak brightness than displays can output, even the best of the best. Even a horrible camera can probably capture much higher peak brightness than like 2.5k nits - which is about the limit of consumer hardware today. 10k is theoretical limit of HDR PQ.
