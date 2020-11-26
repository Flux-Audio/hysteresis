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

// Plugin struct, this is where the processing happens
struct Effect {
    // Store a handle to the plugin's parameter object.
    params: Arc<EffectParameters>,
    rng: Xoshiro256Plus,
    sr: f32,
    T: f32,
    xl_p: f32,
    xr_p: f32,
    xl_q_p: f32,
    xr_q_p: f32,
    yl_p: f32,
    yr_p: f32,
}

// Plugin parameters, this is where the UI happens
struct EffectParameters {
    pre_gain: AtomicFloat,
    bias: AtomicFloat,
    k: AtomicFloat,
    w: AtomicFloat,
    q: AtomicFloat,
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
            sr: 44100.0,    // 
            T: 1.0/44100.0,
            xl_p: 0.0,
            xr_p: 0.0,
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
            pre_gain: AtomicFloat::new(0.5),
            bias: AtomicFloat::new(0.5),
            k: AtomicFloat::new(0.0),
            w: AtomicFloat::new(0.5),
            q: AtomicFloat::new(0.0),
            cut: AtomicFloat::new(0.5),
            post_gain: AtomicFloat::new(0.5),
        }
    }
}

// All plugins using `vst` also need to implement the `Plugin` trait.  Here, we
// define functions that give necessary info to our host.
impl Plugin for Effect {
    fn get_info(&self) -> Info {
        Info {
            name: "Gain Effect in Rust".to_string(),
            vendor: "Rust DSP".to_string(),
            unique_id: 243723072,
            version: 1,
            inputs: 2,
            outputs: 2,
            // This `parameters` bit is important; without it, none of our
            // parameters will be shown!
            parameters: 7,
            category: Category::Effect,
            ..Default::default()
        }
    }

    fn set_sample_rate(&mut self, rate: f32){
        self.sr = rate;
        self.T = 1.0/rate;
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

        // get all params
        let pre_gain = self.params.pre_gain.get()*2.0;
        let bias = self.params.bias.get()*2.0 - 1.0;
        let k = self.params.k.get();
        let w = self.params.w.get();
        let q = self.params.q.get();
        let cut = self.params.cut.get();
        let post_gain = self.params.post_gain.get()*2.0;

        // process
        for ((left_in, right_in), (left_out, right_out)) in stereo_in.zip(stereo_out) {
            // === compute left ================================================
            let xl = *left_in * pre_gain;
            // saturate input
            let dxl = compute::diff(xl, self.xl_p, self.T);
            let xl_sat = compute::sat(xl, dxl, bias, w, k);
            // stochastic quantize
            let xl_quant = compute::x_quant(xl_sat, self.xl_q_p, self.T, q, 
                                            &mut self.rng);
            // output filtering
            let yl = compute::play(xl_quant, self.yl_p, cut);
            // return
            *left_out = yl * post_gain;
            self.xl_p = xl;
            self.xl_q_p = xl_quant;
            self.yl_p = yl;

            // === compute right ===============================================
            let xr = *right_in * pre_gain;
            // saturate input
            let dxr = compute::diff(xr, self.xr_p, self.T);
            let xr_sat = compute::sat(xr, dxr, bias, w, k);
            // stochastic quantize
            let xr_quant = compute::x_quant(xr_sat, self.xr_q_p, self.T, q,
                                            &mut self.rng);
            // output filtering
            let yr = compute::play(xr_quant, self.yr_p, cut);
            // return
            *right_out = yr * post_gain;
            self.xr_p = xr;
            self.xr_q_p = xr_quant;
            self.yr_p = yr;
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
            2 => self.k.get(),
            3 => self.w.get(),
            4 => self.q.get(),
            5 => self.cut.get(),
            6 => self.post_gain.get(),
            _ => 0.0,
        }
    }

    // the `set_parameter` function sets the value of a parameter.
    fn set_parameter(&self, index: i32, val: f32) {
        #[allow(clippy::single_match)]
        match index {
            0 => self.pre_gain.set(val),
            1 => self.bias.set(val),
            2 => self.k.set(val),
            3 => self.w.set(val),
            4 => self.q.set(val),
            5 => self.cut.set(val),
            6 => self.post_gain.set(val),
            _ => (),
        }
    }

    // This is what will display underneath our control.  We can
    // format it into a string that makes the most since.
    fn get_parameter_text(&self, index: i32) -> String {
        match index {
            0 => format!("{:.2}", (self.pre_gain.get()*2.0)),
            1 => format!("{:.2}", (self.bias.get()*2.0 - 1.0)),
            2 => format!("{:.2}", (self.k.get())),
            3 => format!("{:.2}", (self.w.get())),
            4 => format!("{:.2}", (self.q.get())),
            5 => format!("{:.2}", (self.cut.get())),
            6 => format!("{:.2}", (self.post_gain.get()*2.0)),
            _ => "".to_string(),
        }
    }

    // This shows the control's name.
    fn get_parameter_name(&self, index: i32) -> String {
        match index {
            0 => "Pre-gain",
            1 => "Bias",
            2 => "Hysteresis",
            3 => "Hyst. width",
            4 => "Quantum",
            5 => "Post-EQ",
            6 => "Post-gain",
            _ => "",
        }
        .to_string()
    }
}

// This part is important!  Without it, our plugin won't work.
plugin_main!(Effect);