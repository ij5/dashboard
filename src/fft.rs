use std::iter::zip;

use anyhow::Result;
use ndarray::{s, Array, Array1, Array2, Axis};
use ndrustfft::{nddct1, nddct2, ndfft_r2c, Complex, DctHandler, R2cFftHandler};

pub fn safe_log2(x: Array2<f32>) -> Array2<f32> {
    let x = x.mapv(|v| v.min(f32::EPSILON).ln());
    x
}

pub fn safe_log1(x: Array1<f32>) -> Array1<f32> {
    let x = x.mapv(|v| v.min(f32::EPSILON).ln());
    x
}

fn hertz_to_mels(f: f32) -> f32 {
    1127. * (1. + f / 700.).ln()
}

fn mel_to_hertz(mel: Array1<f32>) -> Array1<f32> {
    let a = 700. * ((mel / 1127.).mapv(|v| v.exp()) - 1.);
    a
}

fn correct_grid(x: Array1<i32>) -> Array1<i32> {
    let mut offset = 0;
    let mut list = vec![];
    for (prev, i) in zip(ndarray::array![x[0] - 1] + x.clone(), x) {
        offset = 0.max(offset + prev + 1 - i);
        list.push(i + offset);
    }
    Array::from_vec(list)
}

pub fn filterbanks(sample_rate: u32, num_filt: usize, fft_len: usize) -> Array2<f32> {
    let grid_mels = Array::linspace(
        hertz_to_mels(0.),
        hertz_to_mels(sample_rate as f32),
        num_filt as usize + 2,
    );
    let grid_hertz = mel_to_hertz(grid_mels);
    let grid_indices = (grid_hertz * fft_len as f32 / sample_rate as f32).mapv(|v| v as i32);
    let grid_indices = correct_grid(grid_indices);
    let mut banks = Array2::<f32>::zeros((num_filt, fft_len));
    for (i, data) in chop_array(grid_indices, 3, 1)
        .axis_iter(Axis(0))
        .enumerate()
    {
        banks
            .slice_mut(s![i, data[0]..data[1]])
            .assign(&Array1::linspace(0., 1., (data[1] - data[0]) as usize));
        banks
            .slice_mut(s![i, data[1]..data[2]])
            .assign(&Array1::linspace(1., 0., (data[2] - data[1]) as usize));
    }
    return banks;
}

fn chop_array(arr: Array1<i32>, window_size: usize, hop_size: usize) -> Array2<i32> {
    let mut result = vec![];
    for i in (window_size..arr.len() + 1).step_by(hop_size) {
        result.push(arr.slice(s![i - window_size..i]).to_vec());
    }
    let n1 = arr.len() / hop_size;
    let n2 = window_size;
    let mut data = Vec::with_capacity(n1 * n2);
    for row in &result {
        data.extend_from_slice(&row);
    }
    Array2::from_shape_vec((n1, n2), data).unwrap()
}

pub fn power_spec(audio: Array1<i16>, window_stride: (usize, usize), fft_size: u32) -> Array2<f32> {
    let frames = chop_array(audio.mapv(|v| v as i32), window_stride.0, window_stride.1);
    let frames = frames.mapv(|v| v as f32);

    let shape = frames.shape();
    let mut output = Array2::<Complex<f32>>::zeros((shape[0] / 2 + 1, shape[1]));
    let mut handler = R2cFftHandler::<f32>::new(window_stride.0);
    ndfft_r2c(
        &frames.view(),
        &mut output.view_mut(),
        &mut handler,
        frames.shape().last().unwrap().clone(),
    );
    let result = output.mapv(|v| v.re.powi(2) + v.im.powi(2));
    result / fft_size as f32
}

pub fn mel_spec(
    audio: Array1<i16>,
    sample_rate: u32,
    window_stride: (usize, usize),
    fft_size: u32,
    num_filt: usize,
) -> Array2<f32> {
    let spec = power_spec(audio, window_stride, fft_size);
    let dot = spec.dot(&filterbanks(sample_rate, num_filt, spec.shape()[1]).t());
    safe_log2(dot)
}

pub fn mfcc_spec(
    audio: Array1<i16>,
    sample_rate: u32,
    window_stride: (usize, usize),
    fft_size: u32,
    num_filt: usize,
    num_coeffs: u32,
) -> Array2<f32> {
    let powers = power_spec(audio, window_stride, fft_size);
    if powers.len() == 0 {
        return Array2::zeros((0, num_filt.min(num_coeffs as usize)));
    }
    let filters = filterbanks(sample_rate, num_filt, fft_size as usize);
    let mels = safe_log2(powers.dot(&filters.t()));
    let ny = mels.shape()[1];
    let nx = mels.shape()[0];
    let mut output = Array2::<f32>::zeros((nx, ny));
    let mut handler: DctHandler<f32> = DctHandler::new(ny);
    nddct2(&mels, &mut output, &mut handler, ny);
    let mut output = output.slice(s![.., ..num_coeffs as usize]).to_owned();
    output.slice_mut(s![.., 0]).assign(&safe_log1(powers.sum_axis(Axis(1))));
    output
}
