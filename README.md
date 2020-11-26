# Hysteresis v0.1.0

## Installation
_**Disclaimer:** this plugin will only work on 64-bit windows machines!_ \
Download the `.dll` file in the `/bin` directory and place it into your VST folder.

# What is Hysteresis?

Hysteresis is a tape emulation plugin that models the physical characteristics of ferro-magnetic tape. At its core is a standard hyperbolic tangent saturator, but on top of that it has a few nifty features.

The signal entering the saturator is first integrated with a "hysteresis" function, that emulates the effect of magnetic inertia in the tape. This in practice means that the rising and falling edges of a wave passing through the wave-shaper will be shaped differently, forming a sort of time-asymmetry (notice that this does not create even harmonics, because even harmonics come from phase-asymmetry, not time-asymmetry).

On top of that, the plugin models the tape hiss, not just by adding a boring noise floor, but by emulating the quantum phenomena that make this hiss arise in the real thing. (if you hate hiss, you can turn this off). The phenomena behind this is "stochastic quantization". Essentially the way the tape is magnetized is not in a continuous fashion but through a stepped function with discrete (yet randomly spaced) steps (quanta).

# Controls Explained

- Pre-gain: gain applied before distortion, useful to drive the saturator.
- Bias: constant DC added before the distortion, creates even harmonics. Bipolar control.
- Hysteresis: time-asymmetry of the saturation curve, a little goes a long way.
- Hyst. width: width of the hysteresis window, portion of the saturation wave around the crossover point that is affected by hysteresis. Low-mid values add a nice mid boost.
- Quantum: amount of stochastic quantization, adds correlated noise floor. Interacts in interesting ways with the saturation (higher values of saturation and quantum create glitchy rumbles in the sub range). High values sound like bit-depth reduction. At 0 the effect is off.
- Post-EQ: amount of high-end dampening. This emulates the frequency response of the read heads of a tape machine. The effect is caused by the feedback of the magnetic field onto itself, essentially making the head record the average value over a small portion of the tape rather than an instant value. This is easily emulated by calculating the integrated average of the output.
- Post-gain: gain applied after all the effects, used to tame hard-driven saturation. Try boosting the pre-gain and reducing the post-gain to obtain higher harmonics without considerably changing the perceived loudness.
