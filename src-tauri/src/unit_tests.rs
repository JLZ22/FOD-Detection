#[cfg(test)]
mod tests {
    use image::{DynamicImage, GenericImageView};
    use ndarray::{Array, Axis};
    use rayon::prelude::*;
    use crate::multi_capture;

    #[test]
    fn test_preprocess() {
        let w = 20;
        let h = 20; 
        let fill_val = 144;
        let imgs = vec![multi_capture::pad_to_size(DynamicImage::new_rgb8(2, 4), w, h, fill_val); 3];
        let mut ys_truth = Array::ones((3, 3, h as usize, w as usize)).into_dyn();
        ys_truth.fill(fill_val as f32 / 255.0);
        let mut ys_test = ys_truth.clone();

        for (idx, img) in imgs.iter().enumerate() {
            // confirmed working solution
            for (x, y, rgb) in img.pixels() {
                let x = x as usize;
                let y = y as usize;
                let [r, g, b, _] = rgb.0;
                ys_truth[[idx, 0, y, x]] = (r as f32) / 255.0;
                ys_truth[[idx, 1, y, x]] = (g as f32) / 255.0;
                ys_truth[[idx, 2, y, x]] = (b as f32) / 255.0;
            }
            println!("{:?}\n\n{:?}", img, DynamicImage::new_rgb8(3, 3));
            // parallel solution
            let res = img
                .as_rgb8()
                .expect("valid RGB8")
                .par_iter()
                .map(|&b| (b as f32) / 255.0)
                .collect::<Vec<_>>();

            // resize from 1D to h x w x 3
            let res =
                Array::from_shape_vec((h as usize, w as usize, 3), res).expect("valid matrix");

            let res = res.permuted_axes([2,0,1]); 

            // assign to output array
            ys_test.index_axis_mut(Axis(0), idx).assign(&res);
        }
        assert_eq!(ys_truth, ys_test);
    }
}
