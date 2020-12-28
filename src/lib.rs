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
use std::collections::VecDeque;

mod compute; // contains processing functions

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
            let xo_w = self.params.cross_width.get();
            let bias = self.params.bias.get()*2.0 - 1.0;
            let drive = (5.0*self.params.drive.get().powf(2.0)).exp()*0.5;
            let post_gain = self.params.post_gain.get()*2.0;  // TODO: re-scale

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

            // TODO: / XXX: might not actually do the compression in this release

            // === wow / flutter ===============================================
            // TODO: add option to bypass time effects to remove latency
            // TODO: do the actual modulation

            /*
            let md = 0.0;
            self.dly_line_l.push_back(xl);
            self.dly_line_r.push_back(xr);
            self.dly_line_l.pop_front();
            self.dly_line_r.pop_front();
            xl = *self.dly_line_l.get(2205 + md).unwrap();
            xr = *self.dly_line_r.get(2205 + md).unwrap();
            */

            // === self-erasure ================================================
            xl = self.xl_e_z1 + ((xl - self.xl_e_z1)/erase).tanh()*erase;
            xr = self.xr_e_z1 + ((xr - self.xr_e_z1)/erase).tanh()*erase;
            self.xl_e_z1 = xl;
            self.xr_e_z1 = xr;

            // === dc-decouple =================================================
            xl = xl - self.yl_z1*0.01;
            xr = xr - self.yr_z1*0.01;
            self.yl_z1 = xl;
            self.yr_z1 = xr;

            *left_out = xl*post_gain;
            *right_out = xr*post_gain;

            // === dropouts ====================================================
            // TODO:
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