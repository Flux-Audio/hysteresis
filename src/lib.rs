// VST bindings.
#[macro_use]
extern crate vst;

use vst::buffer::AudioBuffer;
use vst::plugin::{Category, Info, Plugin, PluginParameters};
use vst::util::AtomicFloat;

use std::sync::Arc;

use dsp_lab::emulation::Hysteresis;
use dsp_lab::traits::Process;
use dsp_lab::utils::math::{x_fade};
use dsp_lab::utils::conversion::{db_to_gain};

// mod compute; // contains processing functions

// Plugin struct, this is where the processing happens
struct Effect {
    // Store a handle to the plugin's parameter object.
    params: Arc<EffectParameters>,

    // meta
    // sr: f32,
    // rate: f32,
    hyst_l: Hysteresis,
    hyst_r: Hysteresis,
}

// Plugin parameters, this is where the UI happens
struct EffectParameters {
    pre_post: AtomicFloat,
    dbg_sq: AtomicFloat,
    dbg_coerc: AtomicFloat,
    dry_wet: AtomicFloat,
}

// All plugins using the `vst` crate will either need to implement the `Default`
// trait, or derive from it.  By implementing the trait, we can set a default value.
// Note that controls will always return a value from 0 - 1.  Setting a default to
// 0.5 means it's halfway up.
impl Default for Effect {
    fn default() -> Effect {
        Effect {
            params: Arc::new(EffectParameters::default()),

            // sr: 44100.0,
            // rate: 1.0/44100.0,
            hyst_l: Hysteresis::new(),
            hyst_r: Hysteresis::new(),
        }
    }
}

impl Default for EffectParameters {
    fn default() -> EffectParameters {
        EffectParameters {
            pre_post: AtomicFloat::new(0.5),
            dbg_sq: AtomicFloat::new(0.5),
            dbg_coerc: AtomicFloat::new(0.5),
            dry_wet: AtomicFloat::new(1.0),
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
            unique_id: 0x2d4e04e1,  // adler-32 of name + version (HYSTERESIS v0.3.x)
            version: 30,
            inputs: 2,
            outputs: 2,
            // This `parameters` bit is important; without it, none of our
            // parameters will be shown!
            parameters: 4,
            category: Category::Effect,
            initial_delay: 0,
            ..Default::default()
        }
    }

    /*
    fn set_sample_rate(&mut self, rate: f32){
        self.sr = rate;
        self.rate = 1.0/rate;
    }
    */

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

            // get params
            let sq = self.params.dbg_sq.get();
            let c  = self.params.dbg_coerc.get();
            let pre_post = self.params.pre_post.get() * 24.0 - 12.0;
            let pre  = db_to_gain( pre_post);
            let post = db_to_gain(-pre_post);
            let dry_wet = self.params.dry_wet.get();

            // get inputs
            let mut xl = *left_in * pre;
            let mut xr = *right_in * pre;

            // update process parameters
            self.hyst_l.sq = sq;
            self.hyst_r.sq = sq;
            self.hyst_l.coerc = c;
            self.hyst_r.coerc = c; 

            // execute process chains
            xl = self.hyst_l.step(xl);
            xr = self.hyst_r.step(xr);

            // === out =========================================================
            *left_out  = x_fade(*left_in,  dry_wet, xl * post);
            *right_out = x_fade(*right_in, dry_wet, xr * post);
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
            0 => self.pre_post.get(),
            1 => self.dbg_sq.get(),
            2 => self.dbg_coerc.get(),
            3 => self.dry_wet.get(),
            _ => 0.0,
        }
    }

    // the `set_parameter` function sets the value of a parameter.
    fn set_parameter(&self, index: i32, val: f32) {
        #[allow(clippy::single_match)]
        match index {
            0 => self.pre_post.set(val),
            1 => self.dbg_sq.set(val),
            2 => self.dbg_coerc.set(val),
            3 => self.dry_wet.set(val),
            _ => (),
        }
    }

    // This is what will display underneath our control.  We can
    // format it into a string that makes the most since.
    fn get_parameter_text(&self, index: i32) -> String {
        match index {
            // 0 => format!("{:.2} dB", (self.pre_gain.get()*2.0).powf(2.0).log10()*20.0 ),
            0 => format!("pre: {:.2} dB", 
                self.pre_post.get() *  24.0 - 12.0),
            1 => format!("{:.2}", self.dbg_sq.get()),
            2 => format!("{:.2}", self.dbg_coerc.get()),
            3 => format!("{:.1}% wet", self.dry_wet.get()*100.0),
            _ => "".to_string(),
        }
    }

    // This shows the control's name.
    fn get_parameter_name(&self, index: i32) -> String {
        match index {
            0 => "pre/post gain",
            1 => "squareness",
            2 => "coercitivity",
            3 => "dry/wet",
            _ => "",
        }
        .to_string()
    }
}

// This part is important!  Without it, our plugin won't work.
plugin_main!(Effect);