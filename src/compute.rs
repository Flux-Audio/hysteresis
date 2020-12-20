extern crate rand_xoshiro;

use rand_xoshiro::Xoshiro256Plus;
use rand_xoshiro::rand_core::RngCore;

/// simple RT derivative approximation
pub fn diff(x: f32, x_p: f32, T:f32) -> f32{ return (x - x_p)/T; }


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
pub fn digi_xover(x: f32, amt: f32, w: f32) -> f32{
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


// === HYSTERESIS WINDOW FUNCTION ==============================================

/// digital hysteresis
/// + x     input
/// + amt   amount
/// + w     width
pub fn digital_window(x: f32, amt: f32, w: f32) -> (f32, f32){
    // prepare
    let lim = |x: f32| -> f32 { if x.abs() > 1.0 { x.signum() } else { x }};

    // hysteresis
    let w_p = lim(x + w)*amt + x*(1.0 - amt);
    let w_m = lim(x - w)*amt + x*(1.0 - amt);
    return (w_p, w_m);
}

/// tape hysteresis 1
/// + x     input
/// + amt   amount
/// + asym  asymmetry
pub fn tape_window_1(x: f32, amt: f32, asym: f32) -> (f32, f32){
    // prepare
    let amt_p = (amt*asym*2.0).tanh();
    let amt_m = (amt*(1.0 - asym)*2.0).tanh();

    // hysteresis
    let w_p = (2.82842712*(x + 1.0).sqrt() - 2.0 - x)*amt_p + x*(1.0 - amt_p);
    let w_m = (2.0 - x - 2.82842712*(1.0 - x).sqrt())*amt_m + x*(1.0 - amt_m);
    return (w_p, w_m);
}

/// tape hysteresis 2 ( legacy )
/// + x     input
/// + amt   amount
/// + w     width
/// + dx    delta x
pub fn tape_window_2(x: f32, amt: f32, w: f32, dx: f32) -> f32{
    let _x = 1.0/w*x; // change width of hyp-secant by making the input steeper
    let x_2 = _x*_x;    // pre-compute square of x
    let sech = 24.0/((x_2 + 12.0)*x_2 + 24.0);
    return sech*(w/100.0)*amt*dx;    // scale output for smoother hysteresis
}

/// tube hysteresis
/// + x     input
/// + amt   amount
/// + asym  asymmetry
pub fn tube_window(x: f32, amt: f32, asym: f32) -> (f32, f32){
    let w_p = x - 0.5/(1.0 + 25.0*(x - asym).powf(2.0))*amt;
    let w_m = x + 0.5/(1.0 + 25.0*(-x - asym).powf(2.0))*amt;
    return (w_p, w_m);
} 


// === SATURATION FUNCTION =====================================================

/// tape saturation 1
pub fn tape_sat_1(x: f32) -> f32{
    let sat_1 = if x < -1.4 {
        0.169967143*(x + 1.4) - 0.98544972998
    } else if x > 1.4 {
        0.169967143*(x - 1.4) + 0.98544972998
    } else {
        x.sin()
    };
    return (sat_1*0.8).tanh();
}

/// tape saturation 2 ( legacy )
pub fn tape_sat_2(x: f32) -> f32{
    let x_2 = x*x;  // pre-compute square of x
    return x/(1.0+x_2/(3.0+x_2/(5.0+x_2/(7.0+x_2/13.0))));
}

/// soft clip (transformer saturation)
pub fn soft_clip(x: f32) -> f32{
    return 0.2*((1.0 + (10.0*x + 5.0).exp())/(1.0 + (10.0*x - 5.0).exp())).ln() - 1.0;
}

/// tube saturation
pub fn tupe_sat(x: f32) -> f32{
    let tg = (1.4549654*x).tan()/4.0;
    let sat = (1.4549654*x).tanh();
    let fade = if x.abs() > 1.28741055 { 1.0 } else { 1.0/1.28741055*x.abs() };
    return tg*(fade - 1.0) + sat*fade;
}


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