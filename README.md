# Hysteresis v0.3.0
***Categories:** meta-plugin, circuit modelling*

## Update Notice
Version v0.3.x is incompatible with v0.2.x! Presets cannot be ported. Make sure to
backup any old instances of v0.2.x if you don't want your projects to break.

You will notice that the majority of the controls have been removed since the last
version, this is because I am making new plugins to explore some of the more
esoteric aspects of the previous versions of this plugin, while I wanted to make
the interface of this plugin quite simple. The concept of 
[magnetic hysteresis](https://en.wikipedia.org/wiki/Magnetic_hysteresis) is
now the only topic of interest for this plugin. 

A full changelist is at the bottom of this document.

## Installation
_**Disclaimer:** this plugin will only work on 64-bit windows machines!_ \
Download the `.dll` file in the `bin/` directory and place it into your DAW's VST folder.
Previous versions of the plugin are also available, in case you need them.

## Compiling The Source Code
_**Note:** you don't need to compile the source code if you just want to use the plugin, just download the `.dll`._ \
Make sure you have Cargo installed on your computer (the Rust compiler). Then in the root of the repository run `cargo build`. Once Cargo is done building, there should be a `HYSTERESIS_v0_3_0.dll` file in the newly created `debug/` directory. Place this file into your DAW's VST folder.

# What is Hysteresis?

HYSTERESIS is a plugin modelling how magnetic materials (such as transformer cores
and magnetic tape), react to an incoming magnetic field. In essence, HYSTERESIS
is a form of saturation that is dependent on the previous states of magnetization.

In simpler terms, HYSTERESIS is a weird in-between saturation and strange
unpredictable filtering. It is a major component of what makes analog circuits
sound "warm".

This is **not** a tape emulation, this is more of a test plugin, to develop
physical modelling methods, that I can use in larger projects. This is one plugin in a series of "meta-plugins", which are very simple plugins
that I develop as stepping stones to build more complex plugins in the
future. For example, HYSTERESIS will be used in modelling tape and
transformer saturation for a tape-deck simulator.

# Controls Explained

+ Pre/post gain: positive values boost the pre-gain, and attenuate the post-gain, essentially driving the saturation, without (majorly) affecting the overall loudness. Negative values do the opposite.
+ Squareness: determines the shape of the saturation curve, all the way
down creates a very soft saturation, which sounds quite warm and grungy;
all the way up and it almost turns into hard-clipping. Medium-high values
are suggested for a cleaner sound, as long as the level doesn't make it
clip.
+ Coercitivity: a higher coercitivity means that the metal being 
simulated opposes changes in magnetization, this causes a sort of
distortion that is more prominent on low frequencies at high gain and
high frequencies at low gain. For a quiet signal, it muddies the signal
quite a bit.
+ Dry/wet: self-explanatory.


# Changelist

## v0.3.0
The plugin is completely redesigned from the ground up, and has lost
most of its original functionality, as it has been simplified.
+ Modified: hysteresis model is now way more realistic and optimized.
+ Removed: everything from the previous version.
## v0.2.0
The vast majority of the plugin was re-designed.
- Added: new saturation modes
- Added: Wow and flutter
- Added: Self-erasure effect, emulates old tape where the high frequencies have faded.
- Added: Hiss, introduces multiple layers of correlated and non-correlated noise.
- Added: Dry / wet. Allows flanging and chorus effects.
- Modified: Hysteresis algorithm war reworked and simplified
- Modified: Pre and post-gain faders have been re-scaled and are now showing their values in decibels.
- Removed: post-EQ, now a similar effect can be obtained with erase.
- Removed: hysteresis width. The new hysteresis algorithm does not support this.

# Known Bugs
+ When copying and pasting / duplicating the plugin, the copied plugin will not be initialized correctly by the DAW, or might crash the DAW in certain cases. This also applies 
when copying the plugin indirectly by copying the channel/track the plugin is inside of.
+ On low pre-gain, 32-bit floating point quantization creates small but noticeable artifacts.
