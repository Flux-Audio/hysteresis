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
    rec_nse_l_z1: f32,
    rec_nse_r_z1: f32,
    play_nse_l_z1: f32,
    play_nse_r_z1: f32,

    // audio buffers
    dly_line_l: VecDeque<f32>,
    dly_line_r: VecDeque<f32>,
    dry_line_l: VecDeque<f32>,
    dry_line_r: VecDeque<f32>,
}

// Plugin parameters, this is where the UI happens
struct EffectParameters {
    pre_gain: AtomicFloat,
    bias: AtomicFloat,
    hysteresis: AtomicFloat,
    mode: AtomicFloat,
    drive: AtomicFloat,
    quantum: AtomicFloat,
    wow: AtomicFloat,
    flutter: AtomicFloat,
    erase: AtomicFloat,
    hiss: AtomicFloat,
    dry_wet: AtomicFloat,
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
            rec_nse_l_z1: 0.0,
            rec_nse_r_z1: 0.0,
            play_nse_l_z1: 0.0,
            play_nse_r_z1: 0.0,

            dly_line_l: VecDeque::from(vec![0.0; 4410]),
            dly_line_r: VecDeque::from(vec![0.0; 4410]),
            dry_line_l: VecDeque::from(vec![0.0; 2205]),
            dry_line_r: VecDeque::from(vec![0.0; 2205]),
        }
    }
}

impl Default for EffectParameters {
    fn default() -> EffectParameters {
        EffectParameters {
            pre_gain: AtomicFloat::new(0.5),
            bias: AtomicFloat::new(0.5),
            hysteresis: AtomicFloat::new(0.0),
            mode: AtomicFloat::new(0.0),
            drive: AtomicFloat::new(0.0),
            quantum: AtomicFloat::new(0.0),
            wow: AtomicFloat::new(0.0),
            flutter: AtomicFloat::new(0.0),
            erase: AtomicFloat::new(0.0),
            hiss: AtomicFloat::new(0.0),
            dry_wet: AtomicFloat::new(1.0),
            post_gain: AtomicFloat::new(0.70710678),
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
            parameters: 12,
            category: Category::Effect,
            initial_delay: 2205,
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
            let erase = (1.0 - self.params.erase.get()*0.999).powf(2.0);
            let hyst = self.params.hysteresis.get().powf(3.0);
            let bias = self.params.bias.get()*2.0 - 1.0;
            let drive = (5.0*self.params.drive.get().powf(2.0)).exp()*0.5;
            let pre_gain = (self.params.pre_gain.get()*2.0).powf(2.0);
            let post_gain = self.params.post_gain.get().powf(2.0)*2.0;
            let wow = self.params.wow.get().powf(2.0);
            let flut_amt = self.params.flutter.get().powf(2.0);
            let hiss = self.params.hiss.get().powf(3.0);
            let mode = (self.params.mode.get()*5.0 + 0.5) as u8;
            let dry = 1.0 - self.params.dry_wet.get();
            let wet = self.params.dry_wet.get();

            // === pre-gain ====================================================
            let mut xl = *left_in*pre_gain + bias;
            let mut xr = *right_in*pre_gain + bias;

            // === store dry signal ============================================
            self.dry_line_l.push_back(xl);
            self.dry_line_r.push_back(xr);

            // === recording noise =============================================
            let mut rec_nse_l = (self.rng.next_u64() as f32) / (u64::MAX as f32);
            let mut rec_nse_r = (self.rng.next_u64() as f32) / (u64::MAX as f32);
            // blue-noisify
            rec_nse_l -= self.rec_nse_l_z1*0.5;
            rec_nse_r -= self.rec_nse_r_z1*0.5;
            self.rec_nse_l_z1 = rec_nse_l;
            self.rec_nse_r_z1 = rec_nse_r;
            xl += rec_nse_l*hiss*0.01;
            xr += rec_nse_r*hiss*0.01;

            // === hysteresis ==================================================
            xl = self.xl_h_z1 + compute::analog_xover(xl - self.xl_h_z1, 0.99975, hyst);
            xr = self.xr_h_z1 + compute::analog_xover(xr - self.xr_h_z1, 0.99975, hyst);
            self.xl_h_z1 = xl;
            self.xr_h_z1 = xr;

            // === stochastic quantization =====================================
            xl = compute::x_quant(xl, self.xl_q_z1, self.rate, quantum, &mut self.rng);
            xr = compute::x_quant(xr, self.xr_q_z1, self.rate, quantum, &mut self.rng);
            self.xl_q_z1 = xl;
            self.xr_q_z1 = xr;

            // === saturation ==================================================
            match mode{
                0 => {
                    xl = compute::soft_sat_1(xl*drive)/compute::soft_sat_1(drive) - compute::soft_sat_1(bias*drive)/compute::soft_sat_1(drive);
                    xr = compute::soft_sat_1(xr*drive)/compute::soft_sat_1(drive) - compute::soft_sat_1(bias*drive)/compute::soft_sat_1(drive);
                },
                1 => {
                    xl = compute::mag_sat_1(xl*drive)/compute::mag_sat_1(drive) - compute::mag_sat_1(bias*drive)/compute::mag_sat_1(drive);
                    xr = compute::mag_sat_1(xr*drive)/compute::mag_sat_1(drive) - compute::mag_sat_1(bias*drive)/compute::mag_sat_1(drive);
                },
                2 => {
                    xl = compute::mag_sat_2(xl*drive)/compute::mag_sat_2(drive) - compute::mag_sat_2(bias*drive)/compute::mag_sat_2(drive);
                    xr = compute::mag_sat_2(xr*drive)/compute::mag_sat_2(drive) - compute::mag_sat_2(bias*drive)/compute::mag_sat_2(drive);
                },
                3 => {
                    xl = compute::mag_sat_3(xl*drive)/compute::mag_sat_3(drive) - compute::mag_sat_3(bias*drive)/compute::mag_sat_3(drive);
                    xr = compute::mag_sat_3(xr*drive)/compute::mag_sat_3(drive) - compute::mag_sat_3(bias*drive)/compute::mag_sat_3(drive);
                }
                4 => {
                    xl = compute::mag_sat_4(xl*drive)/compute::mag_sat_4(drive) - compute::mag_sat_4(bias*drive)/compute::mag_sat_4(drive);
                    xr = compute::mag_sat_4(xr*drive)/compute::mag_sat_4(drive) - compute::mag_sat_4(bias*drive)/compute::mag_sat_4(drive);
                },
                5 => {
                    xl = compute::mag_sat_4(xl*drive)/compute::mag_sat_5(drive) - compute::mag_sat_5(bias*drive)/compute::mag_sat_5(drive);
                    xr = compute::mag_sat_4(xr*drive)/compute::mag_sat_5(drive) - compute::mag_sat_5(bias*drive)/compute::mag_sat_5(drive);
                }
                _ => (),
            }
            

            // TODO: voltage-drop compression (planned ver 0.3)

            // === grain noise =================================================
            // emulates the grainy noise generated by the metallic powder grains
            // being of finite resolution, this is not the same as stochastic
            // quantization, which is the noise added by the quantum magnetization
            // steps
            let grain = ((self.rng.next_u64() as f32) / (u64::MAX as f32)).powf(18.0);
            xl += grain*hiss*0.33;
            xr += grain*hiss*0.33;


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
            self.dly_line_l.push_back(xl);
            self.dly_line_r.push_back(xr);
            self.dly_line_l.pop_front();
            self.dly_line_r.pop_front();
            // TODO: proper intersample interpolation (planned ver 0.3)
            // take the integer approximation of playback position and the
            // real value remainder between integer approx and next sample
            let read_idx = 2205.0*my_mod;
            let read_idx_i = read_idx.floor() as usize;
            let read_idx_r =read_idx - (read_idx_i as f32);
            let xl_1 = *self.dly_line_l.get(read_idx_i).unwrap();
            let xr_1 = *self.dly_line_r.get(read_idx_i).unwrap();
            let xl_2 = *self.dly_line_l.get(read_idx_i + 1).unwrap();
            let xr_2 = *self.dly_line_r.get(read_idx_i + 1).unwrap();
            // use remainder of index to crossfade between adjacent samples
            xl = xl_1*(1.0 - read_idx_r) + xl_2*read_idx_r;
            xr = xr_1*(1.0 - read_idx_r) + xr_2*read_idx_r;

            // === self-erasure ================================================
            xl = self.xl_e_z1 + ((xl - self.xl_e_z1)/erase).tanh()*erase;
            xr = self.xr_e_z1 + ((xr - self.xr_e_z1)/erase).tanh()*erase;
            self.xl_e_z1 = xl;
            self.xr_e_z1 = xr;

            // === recording noise =============================================
            let mut play_nse_l = (self.rng.next_u64() as f32) / (u64::MAX as f32);
            let mut play_nse_r = (self.rng.next_u64() as f32) / (u64::MAX as f32);
            // blue-noisify
            play_nse_l -= self.play_nse_l_z1*0.5;
            play_nse_r -= self.play_nse_r_z1*0.5;
            self.play_nse_l_z1 = play_nse_l;
            self.play_nse_r_z1 = play_nse_r;
            xl += play_nse_l*hiss*0.03;
            xr += play_nse_r*hiss*0.03;

            // === dc-decouple =================================================
            // essentially just a high pass filter at 5Hz
            xl = xl - self.yl_z1*0.25;
            xr = xr - self.yr_z1*0.25;
            self.yl_z1 = xl;
            self.yr_z1 = xr;

            // === dry / wet ===================================================
            xl = xl*wet + self.dry_line_l.pop_front().unwrap()*dry;
            xr = xr*wet + self.dry_line_r.pop_front().unwrap()*dry;

            // === out =========================================================
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
            1 => self.bias.get(),
            2 => self.hysteresis.get(),
            3 => self.mode.get(),
            4 => self.drive.get(),
            5 => self.quantum.get(),
            6 => self.wow.get(),
            7 => self.flutter.get(),
            8 => self.erase.get(),
            9 => self.hiss.get(),
            10 => self.dry_wet.get(),
            11 => self.post_gain.get(),
            _ => 0.0,
        }
    }

    // the `set_parameter` function sets the value of a parameter.
    fn set_parameter(&self, index: i32, val: f32) {
        #[allow(clippy::single_match)]
        match index {
            0 => self.pre_gain.set(val),
            1 => self.bias.set(val),
            2 => self.hysteresis.set(val),
            3 => self.mode.set(val),
            4 => self.drive.set(val),
            5 => self.quantum.set(val),
            6 => self.wow.set(val),
            7 => self.flutter.set(val),
            8 => self.erase.set(val),
            9 => self.hiss.set(val),
            10 => self.dry_wet.set(val),
            11 => self.post_gain.set(val),
            _ => (),
        }
    }

    // This is what will display underneath our control.  We can
    // format it into a string that makes the most since.
    fn get_parameter_text(&self, index: i32) -> String {
        match index {
            0 => format!("{:.2} dB", (self.pre_gain.get()*2.0).powf(2.0).log10()*20.0 ),
            1 => format!("{:.2}", (self.bias.get())),
            2 => format!("{:.2}", (self.hysteresis.get())),
            3 => match (self.mode.get()*5.0 + 0.5) as u8{
                0 => String::from("soft"),
                1 => String::from("tungsten"),
                2 => String::from("steel"),
                3 => String::from("iron"),
                4 => String::from("nickel"),
                5 => String::from("magnetite"),
                _ => String::new(),
            },
            4 => format!("{:.2}", (self.drive.get())),
            5 => format!("{:.2}", (self.quantum.get())),
            6 => format!("{:.2}", (self.wow.get())),
            7 => format!("{:.2}", (self.flutter.get())),
            8 => format!("{:.2}", (self.erase.get())),
            9 => format!("{:.2}", (self.hiss.get())),
            10 => format!("{:.2}", self.dry_wet.get()),
            11 => format!("{:.2} dB", (self.post_gain.get().powf(2.0)*2.0).log10()*20.0 ),
            _ => "".to_string(),
        }
    }

    // This shows the control's name.
    fn get_parameter_name(&self, index: i32) -> String {
        match index {
            0 => "pre-gain",
            1 => "bias",
            2 => "hysteresis",
            3 => "sat. mode",
            4 => "drive",
            5 => "quantum",
            6 => "wow",
            7 => "flutter",
            8 => "erase",
            9 => "hiss",
            10 => "dry / wet",
            11 => "post-gain",
            _ => "",
        }
        .to_string()
    }
}

// This part is important!  Without it, our plugin won't work.
plugin_main!(Effect);