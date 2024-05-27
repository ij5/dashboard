use candle_core::{Device, IndexOp, Tensor};
use candle_nn as nn;
use ndarray::{Array, Array1};
use rand::Rng;

use crate::fft::mfcc_spec;

pub struct MFCC {
    sample_rate: u32,
    window_stride: (usize, usize),
    fft_size: u32,
    num_filt: u32,
    num_coeffs: u32,
    device: Device,
}

impl MFCC {
    pub fn new(
        sample_rate: u32,
        window_stride: (usize, usize),
        fft_size: u32,
        num_filt: u32,
        num_coeffs: u32,
    ) -> Self {
        let device = Device::cuda_if_available(0).unwrap();
        Self {
            fft_size,
            num_coeffs,
            num_filt,
            sample_rate,
            window_stride,
            device,
        }
    }
}

impl nn::Module for MFCC {
    fn forward(&self, xs: &candle_core::Tensor) -> candle_core::Result<Tensor> {
        let xs = xs.squeeze(0)?;
        let xs = xs.mean(0)?;
        let data = Array1::from_vec(xs.to_vec1::<f32>()?);
        let data = data.mapv(|v| (v * 32767.0) as i16);
        let mfcc = mfcc_spec(
            data,
            self.sample_rate,
            self.window_stride,
            self.fft_size,
            self.num_filt as usize,
            self.num_coeffs,
        );
        let shape = mfcc.shape();
        let mfcc = Array::from_iter(mfcc.iter().cloned());
        // let data =data.mapv(|v| v)
        let result = Tensor::from_vec(mfcc.to_vec(), shape, &self.device)?;
        Ok(result)
    }
}

pub struct RandomCut {
    max_cut: u32,
    device: Device,
}

impl RandomCut {
    pub fn new(max_cut: u32, device: &Device) -> Self {
        return Self {
            max_cut,
            device: device.clone(),
        }
    }
}

impl nn::Module for RandomCut {
    fn forward(&self, xs: &Tensor) -> candle_core::Result<Tensor> {
        let mut rng = rand::thread_rng();
        let side: u32 = rng.gen_range(0..=1);
        let cut = rng.gen_range(1..=self.max_cut);
        if side == 0 {
            return xs.i((..xs.dims()[0]-cut as usize, .., ..));
        } else {
            return xs.i((cut as usize.., .., ..));
        }
    }
}

pub struct SpecAugment {
    rate: f32,
    specaug: nn::Sequential,
    specaug2: nn::Sequential,
}

impl SpecAugment {
    pub fn new(rate: f32, specaug: nn::Sequential, specaug2: nn::Sequential) -> Self {
        Self { rate, specaug, specaug2 }
    }
}

// impl nn::Module for SpecAugment {
//     fn forward(&self, xs: &Tensor) -> candle_core::Result<Tensor> {
//         let rng = rand::thread_rng();
//         let probability: f64 = rng.gen_range(0.0..1.0);
//         if probability > 0.5 {
//             return 
//         }
//     }
    
// }


pub struct AxisMasking {
    mask_param: u32,
    axis: u32,
    iid_masks: bool,
    p: f32
}

impl AxisMasking {
    pub fn new(mask_param: u32, axis: u32, iid_masks: bool, p: f32) -> Self {
        Self { mask_param, axis, iid_masks, p }
    }
}

impl nn::Module for AxisMasking {
    fn forward(&self, xs: &Tensor) -> candle_core::Result<Tensor> {
        if self.iid_masks {
            return 
        }
    }
}

