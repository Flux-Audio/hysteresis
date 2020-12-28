// VST bindings.

#![feature(tau_constant)]
#[macro_use]
extern crate vst;
extern crate rand_xoshiro;

use vst::buffer::AudioBuffer;
use vst::plugin::{Category, Info, Plugin, PluginParameters};
use vst::util::AtomicFloat;

use rand_xoshiro::rand_core::SeedableRng;
use rand_xoshiro::Xoshiro256Plus;
use rand_xoshiro::rand_core::RngCore;

use std::sync::Arc;
use std::collections::VecDeque;
use std::f32::consts;

mod compute; // contains processing functions

const MOD_RATE: f32 = 0.23;
const LP_1_CUT: f32 = 15.0;
const HP_1_CUT: f32 = 2.5;
const HP_2_CUT: f32 = 500.0;

// Plugin struct, this is where the processing happens
struct Effect {
    // Store a handle to the plugin's parameter object.
    params: Arc<EffectParameters>,

    // meta
    rng: Xoshiro256Plus,
    sr: f32,
    rate: f32,

    // differential variables
    xl_q_z1: f32,
    xr_q_z1: f32,
    xl_h_z1: f32,
    xr_h_z1: f32,
    xl_e_z1: f32,
    xr_e_z1: f32,
    yl_z1: f32,
    yr_z1: f32,

    // oscillator accumulators
    ramp_1: f32,
    ramp_2: f32,
    ramp_mod: f32,

    // filter memory cells
    lp_1_z1: f32,
    lp_1_z2: f32,
    flut_z1: f32,
    hp_1_z1: f32,
    hp_2_z1: f32,

    // audio buffers
    dly_line_l: VecDeque<f32>,
    dly_line_r: VecDeque<f32>,
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

            xl_q_z1: 0.0,
            xr_q_z1: 0.0,
            xl_h_z1: 0.0,
            xr_h_z1: 0.0,
            xl_e_z1: 0.0,
            xr_e_z1: 0.0,
            yl_z1: 0.0,
            yr_z1: 0.0,

            ramp_1: 0.0,
            ramp_2: 0.0,
            ramp_mod: 0.0,

            lp_1_z1: 0.0,
            lp_1_z2: 0.0,
            flut_z1: 0.0,
            hp_1_z1: 0.0,
            hp_2_z1: 0.0,

            dly_line_l: VecDeque::from(vec![0.0; 4410]),
            dly_line_r: VecDeque::from(vec![0.0; 4410]),
        }
    }
}

impl Default for EffectParameters {
    fn default() -> EffectParameters {
        EffectParameters {
            pre_gain: AtomicFloat::new(0.5),    // map -60 dB - +18 dB   c: 0 dB
            drive: AtomicFloat::new(0.0),       // map 0.1 - 5.0
            bias: AtomicFloat::new(0.5),        // map -1 - +1
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
            let quantum = self.params.quantum.get();
            //let xo_amt = self.params.cross_amt.get().sqrt().sqrt().sqrt().sqrt().sqrt().sqrt().sqrt().sqrt();
            let erase = (1.0 - self.params.cross_amt.get()*0.999).powf(2.0);
            let xo_w = self.params.cross_width.get().powf(3.0);
            let bias = self.params.bias.get()*2.0 - 1.0;
            let drive = (5.0*self.params.drive.get().powf(2.0)).exp()*0.5;
            let post_gain = self.params.post_gain.get()*2.0;  // TODO: re-scale
            let wow = self.params.bias_mode.get().powf(2.0);
            let flut_amt = self.params.hyst_amt.get();

            // === hysteresis ==================================================
            let mut xl = *left_in + bias;
            let mut xr = *right_in + bias;
            
            xl = self.xl_h_z1 + compute::analog_xover(xl - self.xl_h_z1, 0.99975, xo_w);
            xr = self.xr_h_z1 + compute::analog_xover(xr - self.xr_h_z1, 0.99975, xo_w);
            self.xl_h_z1 = xl;
            self.xr_h_z1 = xr;

            // === stochastic quantization =====================================
            xl = compute::x_quant(xl, self.xl_q_z1, self.rate, quantum, &mut self.rng);
            xr = compute::x_quant(xr, self.xr_q_z1, self.rate, quantum, &mut self.rng);
            self.xl_q_z1 = xl;
            self.xr_q_z1 = xr;

            // === saturation ==================================================
            xl = compute::mag_sat_4(xl*drive)/compute::mag_sat_4(drive) - compute::mag_sat_4(bias*drive)/compute::mag_sat_4(drive);
            xr = compute::mag_sat_4(xr*drive)/compute::mag_sat_4(drive) - compute::mag_sat_4(bias*drive)/compute::mag_sat_4(drive);

            // TODO: voltage-drop compression (planned ver 0.3)

            // === wow / flutter ===============================================
            // modulating oscillator, used to vary the speed of the two other
            // oscillators. A phasor ramp is generated, it is wrapped between
            // 0 and TAU radians. Then a sine wave is produced by taking the sin
            // of the phasor. The same applies for all other oscillators.
            self.ramp_mod = if self.ramp_mod > std::f32::consts::TAU {
                0.0
            } else {
                self.ramp_mod + consts::TAU*(MOD_RATE/self.sr)
            };
            let sine_mod = self.ramp_mod.sin()*0.5 + 0.5;   // make positive

            // osc 1
            // frequency of osc 1 varies between 0 Hz and 0.9 Hz
            let rate_1 = 0.9*sine_mod;
            self.ramp_1 = if self.ramp_1 > std::f32::consts::TAU {
                0.0
            } else {
                self.ramp_1 + consts::TAU*(rate_1/self.sr)
            };
            let sine_1 = self.ramp_1.sin()*0.1;

            // osc 2
            // frequency of osc 2 varies between 0.35 Hz and 1.35 Hz
            let rate_2 = 1.0*(1.0 - sine_mod) + 0.35;
            self.ramp_2 = if self.ramp_2 > std::f32::consts::TAU {
                0.0
            } else {
                self.ramp_2 + consts::TAU*(rate_2/self.sr)
            };
            let sine_2 = self.ramp_2.sin()*0.1;

            // flutter
            // filtered with a second order lowpass filter at 35 Hz in series with
            // a high pass filter at 8 Hz
            // and in parallel with a high pass filter at 500 Hz
            let omega_1 = (-consts::TAU*(LP_1_CUT/self.sr)).exp();
            let omega_2 = (-consts::TAU*(HP_1_CUT/self.sr)).exp();
            let omega_3 = (-consts::TAU*(HP_2_CUT/self.sr)).exp();
            let mut flutter = (self.rng.next_u64() as f32) / (u64::MAX as f32);
            // LP at 35 Hz
            let flutter_lp = (1.0 - omega_1).powf(2.0)*flutter + 2.0*omega_1*self.lp_1_z1 - omega_1.powf(2.0)*self.lp_1_z2;
            // HP at 8 Hz
            let flutter_hp_1 = (1.0 + omega_2)/2.0*flutter_lp - (1.0 + omega_2)/2.0*self.lp_1_z1 + omega_2*self.hp_1_z1;
            // HP at 500 Hz
            let flutter_hp_2 = (1.0 + omega_3)/2.0*flutter - (1.0 + omega_3)/2.0*self.flut_z1 + omega_3*self.hp_2_z1;
            self.lp_1_z2 = self.lp_1_z1;
            self.lp_1_z1 = flutter_lp;
            self.flut_z1 = flutter;
            self.hp_1_z1 = flutter_hp_1;
            self.hp_2_z1 = flutter_hp_2;
            flutter = flutter_hp_1*16.0 + flutter_hp_2*0.00; // FIXME: maybe add hp 2 after intersample interpolation is introduced
            flutter = flutter*flutter*flutter;



            // add together mod sources (mod is a reserved keyword)
            let my_mod = 1.0 + (sine_1 + sine_2)*wow + flutter*flut_amt;


            // TODO: add option to bypass time effects to remove latency
            // TODO: do the actual modulation
            self.dly_line_l.push_back(xl);
            self.dly_line_r.push_back(xr);
            self.dly_line_l.pop_front();
            self.dly_line_r.pop_front();
            // TODO: intersample interpolation (planned ver 0.3)
            xl = *self.dly_line_l.get((2205.0*my_mod) as usize).unwrap();
            xr = *self.dly_line_r.get((2205.0*my_mod) as usize).unwrap();

            // === self-erasure ================================================
            xl = self.xl_e_z1 + ((xl - self.xl_e_z1)/erase).tanh()*erase;
            xr = self.xr_e_z1 + ((xr - self.xr_e_z1)/erase).tanh()*erase;
            self.xl_e_z1 = xl;
            self.xr_e_z1 = xr;

            // === dc-decouple =================================================
            // essentially just a high pass filter at 5Hz
            xl = xl - self.yl_z1*0.25;
            xr = xr - self.yr_z1*0.25;
            self.yl_z1 = xl;
            self.yr_z1 = xr;

            *left_out = xl*post_gain;
            *right_out = xr*post_gain;

            // === dropouts ====================================================
            // TODO: (planned ver 0.3)
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