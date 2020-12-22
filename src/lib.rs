// VST bindings.

#[macro_use]
extern crate vst;
extern crate rand_xoshiro;

use vst::buffer::AudioBuffer;
use vst::plugin::{Category, Info, Plugin, PluginParameters};
use vst::util::AtomicFloat;

use rand_xoshiro::rand_core::SeedableRng;
use rand_xoshiro::Xoshiro256Plus;

use std::sync::Arc;

mod compute; // contains processing functions

enum BiasMode{  TAPE,    TUBE }
enum CrossMode{ DIGITAL, ANALOG }
enum HystMode{  DIGITAL, TAPE1, TAPE2, TUBE }
enum SatMode{   TAPE1,   TAPE2, CLIP,  TUBE }

// Plugin struct, this is where the processing happens
struct Effect {
    // Store a handle to the plugin's parameter object.
    params: Arc<EffectParameters>,

    // meta
    rng: Xoshiro256Plus,
    sr: f32,
    rate: f32,

    // differential variables
    xl_p: f32,
    xr_p: f32,
    w_p_l_p: f32,
    w_m_l_p: f32,
    w_p_r_p: f32,
    w_m_r_p: f32,
    hyst_l_p: f32,
    hyst_r_p: f32,
    xl_q_p: f32,
    xr_q_p: f32,
    yl_p: f32,
    yr_p: f32,
}

// Plugin parameters, this is where the UI happens
struct EffectParameters {
    pre_gain: AtomicFloat,
    drive: AtomicFloat,
    bias: AtomicFloat,
    bias_mode: AtomicFloat,
    cross_amt: AtomicFloat,
    cross_width: AtomicFloat,
    cross_mode: AtomicFloat,
    hyst_amt: AtomicFloat,
    hyst_param: AtomicFloat,
    hyst_mode: AtomicFloat,
    sat_mode: AtomicFloat,
    quantum: AtomicFloat,
    dry_wet: AtomicFloat,
    cut: AtomicFloat,
    post_gain: AtomicFloat,
}

// All plugins using the `vst` crate will either need to implement the `Default`
// trait, or derive from it.  By implementing the trait, we can set a default value.
// Note that controls will always return a value from 0 - 1.  Setting a default to
// 0.5 means it's halfway up.
impl Default for Effect {
    fn default() -> Effect {
        Effect {
            params: Arc::new(EffectParameters::default()),

            rng: Xoshiro256Plus::seed_from_u64(33186003),
            sr: 44100.0,
            rate: 1.0/44100.0,

            xl_p: 0.0,
            xr_p: 0.0,
            w_p_l_p: 0.0,
            w_m_l_p: 0.0,
            w_p_r_p: 0.0,
            w_m_r_p: 0.0,
            hyst_l_p: 0.0,
            hyst_r_p: 0.0,
            xl_q_p: 0.0,
            xr_q_p: 0.0,
            yl_p: 0.0,
            yr_p: 0.0,
        }
    }
}

impl Default for EffectParameters {
    fn default() -> EffectParameters {
        EffectParameters {
            pre_gain: AtomicFloat::new(0.5),    // map -60 dB - +18 dB   c: 0 dB
            drive: AtomicFloat::new(0.0),       // map 0.1 - 5.0
            bias: AtomicFloat::new(0.0),        // map -1 - +1
            bias_mode: AtomicFloat::new(0.0),   // map enum{tape, tube}
            cross_amt: AtomicFloat::new(0.0),
            cross_width: AtomicFloat::new(0.0),
            cross_mode: AtomicFloat::new(0.0),  // map enum{digital, analog}
            hyst_amt: AtomicFloat::new(0.0),
            hyst_param: AtomicFloat::new(0.5),
            hyst_mode: AtomicFloat::new(0.0),   // map enum{digi, tape1, tape2, tube},
            sat_mode: AtomicFloat::new(0.0),    // map enum{tape1, tape2, clip, tube},
            quantum: AtomicFloat::new(0.0),
            dry_wet: AtomicFloat::new(0.0),
            cut: AtomicFloat::new(0.0),
            post_gain: AtomicFloat::new(0.5),   // map -60 dB - +6 dB    c: 0 dB
        }
    }
}

// All plugins using `vst` also need to implement the `Plugin` trait.  Here, we
// define functions that give necessary info to our host.
impl Plugin for Effect {
    fn get_info(&self) -> Info {
        Info {
            name: "HYSTERESIS".to_string(),
            vendor: "Rust DSP".to_string(),
            unique_id: 243723072,
            version: 020,
            inputs: 2,
            outputs: 2,
            // This `parameters` bit is important; without it, none of our
            // parameters will be shown!
            parameters: 15,
            category: Category::Effect,
            ..Default::default()
        }
    }

    fn set_sample_rate(&mut self, rate: f32){
        self.sr = rate;
        self.rate = 1.0/rate;
    }

    // Here is where the bulk of our audio processing code goes.
    fn process(&mut self, buffer: &mut AudioBuffer<f32>) {
        let (inputs, outputs) = buffer.split();

        // Iterate over inputs as (&f32, &f32)
        let (l, r) = inputs.split_at(1);
        let stereo_in = l[0].iter().zip(r[0].iter());

        // Iterate over outputs as (&mut f32, &mut f32)
        let (mut l, mut r) = outputs.split_at_mut(1);
        let stereo_out = l[0].iter_mut().zip(r[0].iter_mut());

        // process
        for ((left_in, right_in), (left_out, right_out)) in stereo_in.zip(stereo_out) {

            // === get all params ==============================================
            let pre_gain = self.params.pre_gain.get().powf(3.0)*8.0;
            let drive = self.params.drive.get()*4.9 + 0.1;
            let bias = self.params.bias.get()*2.0 - 1.0;
            let bias_mode = if self.params.bias_mode.get() < 0.5 {
                BiasMode::TAPE
            } else {
                BiasMode::TUBE
            };
            let cross_amt = self.params.cross_amt.get();
            let cross_width = self.params.cross_width.get();
            let cross_mode = if self.params.cross_mode.get() < 0.5 {
                CrossMode::DIGITAL
            } else {
                CrossMode::ANALOG
            };
            let hyst_amt = self.params.hyst_amt.get().powf(3.0);
            let hyst_param = self.params.hyst_param.get();
            let hyst_mode = match self.params.hyst_mode.get() {
                0.0..=0.25 => HystMode::DIGITAL,
                0.25..=0.5 => HystMode::TAPE1,
                0.5..=0.75 => HystMode::TAPE2,
                _ => HystMode::TUBE
            };
            let quantum = self.params.quantum.get();
            let sat_mode = match self.params.sat_mode.get() {
                0.0..=0.25 => SatMode::TAPE1,
                0.25..=0.5 => SatMode::TAPE2,
                0.5..=0.75 => SatMode::CLIP,
                _ => SatMode::TUBE
            };
            let dry_wet = self.params.dry_wet.get();
            let cut = self.params.cut.get().sqrt().sqrt();
            let post_gain = self.params.post_gain.get()*2.0;

            // === pre-gain and drive ==========================================
            let dry_l = *left_in*pre_gain;
            let dry_r = *right_in*pre_gain;
            let mut xl = dry_l*drive;
            let mut xr = dry_r*drive;

            // === crossover ===================================================
            match cross_mode {
                CrossMode::DIGITAL => {
                    xl = compute::digital_xover(xl, cross_amt, cross_width);
                    xr = compute::digital_xover(xr, cross_amt, cross_width);
                },
                CrossMode::ANALOG => {
                    xl = compute::analog_xover(xl, cross_amt, cross_width);
                    xr = compute::analog_xover(xr, cross_amt, cross_width);
                }
            }

            // === hysteresis ==================================================
            // HACK: legacy functionality implemented by returning tape 2 in the
            // first half of the tuple
            let (w_p_l, w_m_l) = match hyst_mode {
                HystMode::DIGITAL => compute::digital_window(xl, hyst_amt, hyst_param),
                HystMode::TAPE1 => compute::tape_window_1(xl, hyst_amt, hyst_param),
                HystMode::TAPE2 => (compute::tape_window_2(xl, hyst_amt, hyst_param, 
                    compute::diff(xl, self.xl_p, self.rate)), 0.0),
                HystMode::TUBE => compute::tube_window(xl, hyst_amt, hyst_param),
                _ => (0.0, 0.0)
            };
            let (w_p_r, w_m_r) = match hyst_mode {
                HystMode::DIGITAL => compute::digital_window(xr, hyst_amt, hyst_param),
                HystMode::TAPE1 => compute::tape_window_1(xr, hyst_amt, hyst_param),
                HystMode::TAPE2 => (compute::tape_window_2(xr, hyst_amt, hyst_param, 
                    compute::diff(xr, self.xr_p, self.rate)), 0.0),
                HystMode::TUBE => compute::tube_window(xr, hyst_amt, hyst_param),
                _ => (0.0, 0.0)
            };

            // treat legacy mode separately
            match hyst_mode{
                HystMode::TAPE2 => {
                    xl = w_p_l;
                    xr = w_p_r;
                },
                _ => {
                    // differentiate window functions
                    let d_w_p_l = w_p_l - self.w_p_l_p;
                    let d_w_m_l = w_m_l - self.w_m_l_p;
                    let d_w_p_r = w_p_r - self.w_p_r_p;
                    let d_w_m_r = w_m_r - self.w_m_r_p;
                    self.w_p_l_p = w_p_l;
                    self.w_m_l_p = w_m_l;
                    self.w_p_r_p = w_p_r;
                    self.w_m_r_p = w_m_r;

                    // differentiate input
                    let d_xl = xl - self.xl_p;
                    let d_xr = xr - self.xr_p;
                    self.xl_p = xl;
                    self.xr_p = xr;

                    // transfer window delta to x delta
                    let hyst_l: f32;
                    let hyst_r: f32;
                    if d_xl > 0.0{
                        hyst_l = self.hyst_l_p + d_w_p_l;
                    } else {
                        hyst_l = self.hyst_l_p + d_w_m_l;
                    }
                    if d_xr > 0.0{
                        hyst_r = self.hyst_l_p + d_w_p_r;
                    } else {
                        hyst_r = self.hyst_r_p + d_w_m_r;
                    }
                    self.hyst_l_p = hyst_l;
                    self.hyst_r_p = hyst_r;

                    xl = hyst_l;
                    xr = hyst_r;
                }
            }

            // === stochastic quantization =====================================
            xl = compute::x_quant(xl, self.xl_p, self.rate, quantum, &mut self.rng);
            xr = compute::x_quant(xr, self.xr_p, self.rate, quantum, &mut self.rng);

            // === saturation ==================================================
            let mut anti_drive: f32 = 0.0;  // saturate the drive input to scale 
                                            // the output back to unity gain
            match sat_mode{
                SatMode::TAPE1 => { 
                    xl = compute::tape_sat_1(xl); 
                    xr = compute::tape_sat_1(xr);
                    anti_drive = 1.0/compute::tape_sat_1(drive);
                },
                SatMode::TAPE2 => {
                    xl = compute::tape_sat_2(xl); 
                    xr = compute::tape_sat_2(xr);
                    anti_drive = 1.0/compute::tape_sat_2(drive);
                },
                SatMode::TUBE => {
                    xl = compute::tube_sat(xl); 
                    xr = compute::tube_sat(xr);
                    anti_drive = 1.0/compute::tube_sat(drive);
                },
                SatMode::CLIP => {
                    xl = compute::soft_clip(xl); 
                    xr = compute::soft_clip(xr);
                    anti_drive = 1.0/compute::soft_clip(drive);
                },
                _ => ()
            }

            // === anti-drive ==================================================
            xl *= anti_drive;
            xr *= anti_drive;

            // === dry / wet ===================================================
            xl = xl*dry_wet + dry_l*(1.0 - dry_wet);
            xr = xr*dry_wet + dry_r*(1.0 - dry_wet);

            // === post-eq =====================================================
            xl = compute::play(xl, self.yl_p, cut);
            xr = compute::play(xr, self.yr_p, cut);
            self.yl_p = xl;
            self.yr_p = xr;

            // === post gain ===================================================
            let yl = xl*post_gain;
            let yr = xr*post_gain;

            *left_out = yl;
            *right_out = yr;
        }
    }

    // Return the parameter object. This method can be omitted if the
    // plugin has no parameters
    fn get_parameter_object(&mut self) -> Arc<dyn PluginParameters> {
        Arc::clone(&self.params) as Arc<dyn PluginParameters>
    }
}

impl PluginParameters for EffectParameters {
    // the `get_parameter` function reads the value of a parameter.
    fn get_parameter(&self, index: i32) -> f32 {
        match index {
            0 => self.pre_gain.get(),
            1 => self.drive.get(),
            2 => self.bias.get(),
            3 => self.bias_mode.get(),
            4 => self.cross_amt.get(),
            5 => self.cross_width.get(),
            6 => self.cross_mode.get(),
            7 => self.hyst_amt.get(),
            8 => self.hyst_param.get(),
            9 => self.hyst_mode.get(),
            10 => self.sat_mode.get(),
            11 => self.quantum.get(),
            12 => self.dry_wet.get(),
            13 => self.cut.get(),
            14 => self.post_gain.get(),
            _ => 0.0,
        }
    }

    // the `set_parameter` function sets the value of a parameter.
    fn set_parameter(&self, index: i32, val: f32) {
        #[allow(clippy::single_match)]
        match index {
            0 => self.pre_gain.set(val),
            1 => self.drive.set(val),
            2 => self.bias.set(val),
            3 => self.bias_mode.set(val),
            4 => self.cross_amt.set(val),
            5 => self.cross_width.set(val),
            6 => self.cross_mode.set(val),
            7 => self.hyst_amt.set(val),
            8 => self.hyst_param.set(val),
            9 => self.hyst_mode.set(val),
            10 => self.sat_mode.set(val),
            11 => self.quantum.set(val),
            12 => self.dry_wet.set(val),
            13 => self.cut.set(val),
            14 => self.post_gain.set(val),
            _ => (),
        }
    }

    // This is what will display underneath our control.  We can
    // format it into a string that makes the most since.
    fn get_parameter_text(&self, index: i32) -> String {
        match index {
            0 => format!("{:.2}", (self.pre_gain.get()*2.0)), // TODO: dB
            1 => format!("{:.2}", (self.drive.get())),
            2 => format!("{:.2}", (self.bias.get())),
            3 => format!("{:.2}", (self.bias_mode.get())),
            4 => format!("{:.2}", (self.cross_amt.get())),
            5 => format!("{:.2}", (self.cross_width.get())),
            6 => format!("{:.2}", (self.cross_mode.get())),
            7 => format!("{:.2}", (self.hyst_amt.get())),
            8 => format!("{:.2}", (self.hyst_param.get())),
            9 => format!("{:.2}", (self.hyst_mode.get())),
            10 => format!("{:.2}", (self.sat_mode.get())),
            11 => format!("{:.2}", (self.quantum.get())),
            12 => format!("{:.2}", (self.dry_wet.get())),
            13 => format!("{:.2}", (self.cut.get())),
            14 => format!("{:.2}", (self.post_gain.get())),
            _ => "".to_string(),
        }
    }

    // This shows the control's name.
    fn get_parameter_name(&self, index: i32) -> String {
        match index {
            0 => "Pre-gain",
            1 => "Drive",
            2 => "Bias",
            3 => "Bias Mode",
            4 => "Crossover",
            5 => "Cross. Width",
            6 => "Cross. Mode",
            7 => "Hysteresis",
            8 => "Hyst. Warp",
            9 => "Hyst. Mode",
            10 => "Saturation",
            11 => "Quantum",
            12 => "Dry / Wet",
            13 => "Post-EQ",
            14 => "Post-gain",
            _ => "",
        }
        .to_string()
    }
}

// This part is important!  Without it, our plugin won't work.
plugin_main!(Effect);