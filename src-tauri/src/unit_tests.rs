#[cfg(test)]
mod tests {
    use image::{DynamicImage, GenericImageView};
    use ndarray::{Array, Axis};
    use rayon::prelude::*;

    #[test]
    fn test_preprocess() {
        let imgs = vec![DynamicImage::new_rgb8(4, 4); 3];
        let fill_val = 144.0 / 255.0;
        let mut ys_truth = Array::ones((3, 3, 10, 10)).into_dyn();
        ys_truth.fill(fill_val);
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

            // parallel solution
            let mut res = img
                .as_rgb8()
                .expect("valid RGB8")
                .par_iter()
                .map(|&b| (b as f32) / 255.0)
                .collect::<Vec<_>>();

            // add filler pixels as padding
            let flattened_diff = 3 * 10 * 10 - res.len() as u32;
            res.extend(vec![fill_val; flattened_diff as usize]);

            // resize from 1D to h x w x 3
            let res =
                Array::from_shape_vec((10 as usize, 10 as usize, 3), res).expect("valid matrix");

            // resize from h x w x 3 to 3 x h x w
            let res = res
                .into_shape((3, 10 as usize, 10 as usize))
                .expect("valid reshape");

            // assign to output array
            ys_test.index_axis_mut(Axis(0), idx).assign(&res);
        }
        assert_eq!(ys_truth, ys_test);
    }
}
