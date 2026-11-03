#![forbid(unsafe_code)]

use craft_fft::{Complex, ComplexFft, Direction, Imdct, Mdct, RealFft, Sample, Stft, TdacWindow};

fn allocation_free<T: Sample>(length: usize) -> Result<(), craft_fft::Error> {
    let zero = T::scalar(0.0);
    let mut complex = ComplexFft::<T>::new(length)?;
    let mut complex_input = vec![Complex::new(T::scalar(1.0), zero); length];
    let mut complex_output = complex_input.clone();
    let mut real = RealFft::<T>::new(length)?;
    let real_input = vec![T::scalar(1.0); length];
    let mut real_output = real_input.clone();
    let mut spectrum = vec![Complex::new(zero, zero); real.spectrum_len()];
    let mut mdct = Mdct::<T>::new(2 * length, TdacWindow::Sine)?;
    let mut imdct = Imdct::<T>::new(2 * length, TdacWindow::Kbd { alpha: 4.0 })?;
    let mdct_input = vec![T::scalar(1.0); 4 * length];
    let mut coefficients = vec![zero; 2 * length];
    let mut mdct_output = mdct_input.clone();
    let mut stft = Stft::<T>::new(&real_input, 1 + length / 3)?;
    let mut hop = vec![zero; stft.hop()];
    let counts = allocation_counter::measure(|| {
        for _ in 0..8 {
            assert_eq!(complex.process_in_place(Direction::Forward, &mut complex_input), Ok(()));
            assert_eq!(complex.process_into(Direction::Inverse, &complex_input, &mut complex_output), Ok(()));
            assert_eq!(real.forward_into(&real_input, &mut spectrum), Ok(()));
            assert_eq!(real.inverse_into(&spectrum, &mut real_output), Ok(()));
            real.time_mut().copy_from_slice(&real_input);
            assert_eq!(real.process_in_place(Direction::Forward), Ok(()));
            assert_eq!(real.process_in_place(Direction::Inverse), Ok(()));
            assert_eq!(mdct.process_into(&mdct_input, &mut coefficients), Ok(()));
            assert_eq!(imdct.process_into(&coefficients, &mut mdct_output), Ok(()));
            assert_eq!(stft.analyze_into(&real_input, &mut spectrum), Ok(()));
            assert_eq!(stft.synthesize_into(&spectrum, &mut hop), Ok(()));
            assert!(stft.flush_into(&mut hop).is_ok());
            stft.reset();
        }
    });
    assert_eq!(counts.count_total, 0, "processing allocated at N={length}");
    Ok(())
}

#[test]
fn all_processing_is_allocation_free_including_first_call() -> Result<(), craft_fft::Error> {
    for length in [1, 2, 31, 64, 120, 128, 240, 480, 960, 1024, 8192] {
        allocation_free::<f32>(length)?;
        allocation_free::<f64>(length)?;
    }
    Ok(())
}
