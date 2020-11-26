extern crate rand_xoshiro;

use rand_xoshiro::Xoshiro256Plus;
use rand_xoshiro::rand_core::RngCore;

/// simple RT derivative approximation
pub fn diff(x: f32, x_p: f32, T:f32) -> f32{ return (x - x_p)/T; }

// variable width hyp-secant (fast) approximation (hysteresis function)
fn sech(x: f32, w: f32, amt: f32) -> f32{
    let _x = 1.0/w*x; // change width of hyp-secant by making the input steeper
    let x_2 = _x*_x;    // pre-compute square of x
    let sech = 24.0/((x_2 + 12.0)*x_2 + 24.0);
    return sech*(w/100.0)*amt;    // scale output for smoother hysteresis
}

// hyp-tangent (fast) approximation (saturation function)
fn tanh(x: f32) -> f32{
    let x_2 = x*x;  // pre-compute square of x
    return x/(1.0+x_2/(3.0+x_2/(5.0+x_2/(7.0+x_2/13.0))));
}

/// tape saturation
/// + x     input
/// + dx    derivative of input
/// + bias  input bias (asymmetric distortion)
/// + w     hysteresis width
/// + k     hysteresis depth
pub fn sat(x: f32, dx: f32, bias: f32, w: f32, k: f32) -> f32{
    return tanh((x + bias)*0.8 + sech(x, w, k)*dx) - bias/1.316;
}

/// stochastic quantization
///     simulates quantum nature of magnetic tape magnetization
/// + x:    input
/// + y_p:  previous output
/// + T:    intersample period
/// + q:    quantization amount
/// + rng:  reference to random number generator
pub fn x_quant(x: f32, x_p: f32, T: f32, 
                q: f32, rng: &mut Xoshiro256Plus) -> f32{
    let dx = diff(x, x_p, T);
    let _dx = dx.abs();
    let r = (rng.next_u64() as f32) / (u64::MAX as f32);
    if (r < (_dx*(1.0 - q).powf(T*44100.0*8.0))){
        return x;
    }
    return x_p;
}

/// playback head frequency response
///     simulates the playback head not picking up high frequencies
/// + x     : input
/// + y_p   : previous output
/// + cut   : cutoff (as proportion of nyquist limit)
pub fn play(x: f32, y_p: f32, cut: f32) -> f32{
    return x*(1.0 - cut) + y_p*cut;
}