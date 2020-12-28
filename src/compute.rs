extern crate rand_xoshiro;

use rand_xoshiro::Xoshiro256Plus;
use rand_xoshiro::rand_core::RngCore;
use std::f32::consts;

/// simple RT derivative approximation
pub fn diff(x: f32, x_p: f32, rate:f32) -> f32{ return (x - x_p)/rate; }


// === BIAS FUNCTION ===========================================================

/// tube bias (swish function)
pub fn tube_bias(x: f32, bias: f32) -> f32{
    return x*(2.0 - bias/4.0)/(1.0 + (-bias*x).exp());
}


// === CROSSOVER FUNCTION ======================================================

/// digital crossover
/// + x     input
/// + amt   amount
/// + w     width
pub fn digital_xover(x: f32, amt: f32, w: f32) -> f32{
    // TODO: it don't work
    return x - (if x.abs() < w { 
        x/(amt.atanh() + 1.0) 
    } else {
        x.signum()*w/(amt.atanh() + 1.0)
    });
}

/// analog crossover
/// + x     input
/// + amt   amount
/// + w     width
pub fn analog_xover(x: f32, amt: f32, w: f32) -> f32{
    // prepare
    let soft = 1.0 - amt;
    let trans = |x: f32| -> f32 {
        (2.0*w + soft - 2.82842712*(soft*(w - x)).sqrt())/2.0
    };
    let x_abs = x.abs();

    // crossover
    return if x_abs < w - soft/2.0 {
        (trans(x_abs) - trans(0.0))*x.signum()
    } else {
        (x_abs - trans(0.0))*x.signum()
    };
}


// === HYSTERESIS ==============================================================

// === SATURATION FUNCTION =====================================================

/// tungsten magnetic saturation
pub fn mag_sat_1 (x: f32) -> f32 { (x*x*x*1.6 + x*0.4).tanh() }

/// steel magnetic saturation
pub fn mag_sat_2 (x: f32) -> f32 { (x*x*x*3.0 + x*0.75).atan()*consts::FRAC_2_PI }

/// iron magnetic saturation
pub fn mag_sat_3 (x: f32) -> f32 { (x*1.6).atan()*consts::FRAC_2_PI }

/// nickel magnetic saturation
pub fn mag_sat_4 (x: f32) -> f32 { x.tanh() }

/// magnetite magnetic saturation
pub fn mag_sat_5 (x: f32) -> f32 { (3.0*x.powf(1.8)).atan()*consts::FRAC_2_PI }


// === QUANTIZATION FUNCTION ===================================================

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
    if r < (_dx*(1.0 - q).powf(T*44100.0*8.0)){
        return x;
    }
    return x_p;
}


// === FILTER FUNCTION =========================================================

/// playback head frequency response
///     simulates the playback head not picking up high frequencies
/// + x     : input
/// + y_p   : previous output
/// + cut   : cutoff (as proportion of nyquist limit)
pub fn play(x: f32, y_p: f32, cut: f32) -> f32{
    return x*(1.0 - cut) + y_p*cut;
}