#![allow(clippy::type_complexity)]

use std::io::{Cursor, Read, Write};
use std::path::Path;

pub mod args;
pub mod model;
pub mod ort_backend;
pub mod yolo_result;
pub use crate::args::Args;
pub use crate::model::YOLOv8;
pub use crate::ort_backend::{Batch, OrtBackend, OrtConfig, OrtEP, YOLOTask};
pub use crate::yolo_result::{Bbox, Embedding, Point2, YOLOResult};
use opencv::{videoio, prelude::*};
use image::{DynamicImage, ImageOutputFormat};
use mat2image::ToImage;

pub fn non_max_suppression(
    xs: &mut Vec<(Bbox, Option<Vec<Point2>>, Option<Vec<f32>>)>,
    iou_threshold: f32,
) {
    xs.sort_by(|b1, b2| b2.0.confidence().partial_cmp(&b1.0.confidence()).unwrap());

    let mut current_index = 0;
    for index in 0..xs.len() {
        let mut drop = false;
        for prev_index in 0..current_index {
            let iou = xs[prev_index].0.iou(&xs[index].0);
            if iou > iou_threshold {
                drop = true;
                break;
            }
        }
        if !drop {
            xs.swap(current_index, index);
            current_index += 1;
        }
    }
    xs.truncate(current_index);
}

pub fn gen_time_string(delimiter: &str) -> String {
    let offset = chrono::FixedOffset::east_opt(8 * 60 * 60).unwrap(); // Beijing
    let t_now = chrono::Utc::now().with_timezone(&offset);
    let fmt = format!(
        "%Y{}%m{}%d{}%H{}%M{}%S{}%f",
        delimiter, delimiter, delimiter, delimiter, delimiter, delimiter
    );
    t_now.format(&fmt).to_string()
}

pub const SKELETON: [(usize, usize); 16] = [
    (0, 1),
    (0, 2),
    (1, 3),
    (2, 4),
    (5, 6),
    (5, 11),
    (6, 12),
    (11, 12),
    (5, 7),
    (6, 8),
    (7, 9),
    (8, 10),
    (11, 13),
    (12, 14),
    (13, 15),
    (14, 16),
];

pub fn check_font(font: &str) -> rusttype::Font<'static> {
    // check then load font

    // ultralytics font path
    let font_path_config = match dirs::config_dir() {
        Some(mut d) => {
            d.push("Ultralytics");
            d.push(font);
            d
        }
        None => panic!("Unsupported operating system. Now support Linux, MacOS, Windows."),
    };

    // current font path
    let font_path_current = std::path::PathBuf::from(font);

    // check font
    let font_path = if font_path_config.exists() {
        font_path_config
    } else if font_path_current.exists() {
        font_path_current
    } else {
        println!("Downloading font...");
        let source_url = "https://ultralytics.com/assets/Arial.ttf";
        let resp = ureq::get(source_url)
            .timeout(std::time::Duration::from_secs(500))
            .call()
            .unwrap_or_else(|err| panic!("> Failed to download font: {source_url}: {err:?}"));

        // read to buffer
        let mut buffer = vec![];
        let total_size = resp
            .header("Content-Length")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap();
        let _reader = resp
            .into_reader()
            .take(total_size)
            .read_to_end(&mut buffer)
            .unwrap();

        // save
        let _path = std::fs::File::create(font).unwrap();
        let mut writer = std::io::BufWriter::new(_path);
        writer.write_all(&buffer).unwrap();
        println!("Font saved at: {:?}", font_path_current.display());
        font_path_current
    };

    // load font
    let buffer = std::fs::read(font_path).unwrap();
    rusttype::Font::try_from_vec(buffer).unwrap()
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("Failed to load image: {0}")]
    Image(#[from] image::ImageError),

    #[error("Failed to load model: {0}")]
    Model(#[from] ort::OrtError),
}

/*
1s for (4000, 3000) image
0.35s for (2622, 1748) image
*/
pub fn get_img_from_path(path: &Path) -> Result<DynamicImage, LoadError> {
    let img = image::open(path)?;
    img.to_rgb8();
    Ok(img)
}

/*
With rbase64::encode
    2.4s for (4000, 3000) image
        read time: 2.3989345 s, base64 time: 0.013330333 s
    0.9s for (2622, 1748) image

With general_purpose::STANDARD.encode
    2.4s for (4000, 3000) image
        read time: 2.36815725 s, base64 time: 0.04491725 s
    0.9s for (2622, 1748) image
*/
pub fn image_to_base64(img: &DynamicImage) -> String {
    let mut image_data: Vec<u8> = Vec::new();
    img.write_to(&mut Cursor::new(&mut image_data), ImageOutputFormat::WebP)
        .unwrap(); // change to err handle
                   // let res_base64 = general_purpose::STANDARD.encode(image_data);
    let res_base64 = rbase64::encode(&image_data);
    format!("data:image/png;base64,{}", res_base64)
}


fn get_cameras() -> Vec<videoio::VideoCapture> {
    let mut cameras = Vec::new();

    for i in 0..5 {
        let cam = videoio::VideoCapture::new(i, videoio::CAP_ANY).unwrap();

        if cam.is_opened().unwrap() {
            cameras.push(cam);
        }
    }

    cameras
}

#[tauri::command]
fn start_streaming() -> Vec<String> {
    let imgs = (0..=2)
        .map(|i| get_img_from_path(Path::new(&format!("./resources/images/person{}.jpg", i))).unwrap())
        .collect::<Vec<_>>();
    let mut model = YOLOv8::new(Args::new_from_toml(Path::new("./model_args.toml"))).unwrap();
    let results = model.run(&imgs).unwrap();

    model.plot_batch(&results, &imgs, None)
    .iter()
    .map(|img| image_to_base64(&DynamicImage::ImageRgb8(img.clone())))
    .collect::<Vec<_>>().clone()
}

#[tauri::command]
fn update_win_camera(win: i32, index: i32) -> bool {
    // TODO
    return true;
}

#[tauri::command]
fn poll_and_emit_image_sources(window: tauri::Window) {
    // TODO emit a message if the list of cameras changes (implement a frontend handler for this)
    println!("polling and emitting image sources");
    tauri::async_runtime::spawn(async move {
        let mut vals = vec![];
        loop {
            vals.push(vals.len());
            window.emit("available-cameras", vals.clone()).unwrap();
            println!("emitted {:?}", vals);
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    });
}

pub fn run() {
    tauri::Builder::default()
        .setup(|_app| {
            // grab cameras and start inference

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![start_streaming, poll_and_emit_image_sources, update_win_camera])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
