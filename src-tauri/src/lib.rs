#![allow(clippy::type_complexity)]

use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub mod args;
pub mod model;
pub mod ort_backend;
pub mod yolo_result;
pub use crate::args::Args;
pub use crate::model::YOLOv8;
pub use crate::ort_backend::{Batch, OrtBackend, OrtConfig, OrtEP, YOLOTask};
pub use crate::yolo_result::{Bbox, Embedding, Point2, YOLOResult};
use image::{DynamicImage, ImageFormat};
use mat2image::ToImage;
use opencv::{prelude::*, videoio};

const NUM_CAMERAS: usize = 3;

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
    img.write_to(&mut Cursor::new(&mut image_data), ImageFormat::WebP)
        .unwrap(); // change to err handle
                   // let res_base64 = general_purpose::STANDARD.encode(image_data);
    let res_base64 = rbase64::encode(&image_data);
    format!("data:image/png;base64,{}", res_base64)
}

// Binary search for the maximum camera index that is available
// l should always be 0
fn get_camera_indices() -> Vec<i32> {
    let start = Instant::now();
    let mut indices = vec![];
    for i in 0..5 {
        let mut cap = videoio::VideoCapture::new(i, videoio::CAP_ANY).unwrap();
        if cap.is_opened().unwrap() {
            indices.push(i);
            cap.release().unwrap();
        }
    }
    println!("get_camera_indices: {:?}", start.elapsed());
    indices
}

// Get a frame from a video capture object and convert it to a DynamicImage
fn get_frame_from_cap(cap: &mut videoio::VideoCapture) -> Option<DynamicImage> {
    let mut img = Mat::default();
    if cap.read(&mut img).unwrap_or(false) {
        match img.to_image_par() {
            Ok(image) => return Some(image),
            Err(_) => return None,
        }
    } else {
        return None;
    }
}

#[tauri::command]
fn update_camera(window: tauri::Window, win_index: i32, cam_index: i32) {
    window.trigger(
        "update-camera",
        Some(format!("{win_index} {cam_index}").to_string()),
    );
}

/*
Times for 3 video capture objects using Mac FaceTime HD Camera:
    Initial camera elapsed: 1.427801208s
    Event handler elapsed: 0ns
    Get frame elapsed: 103.224708ms
    Inference elapsed: 2.240577042s
    Plot elapsed: 185.965083ms
    Base64 elapsed: 1.199653625s
*/
#[tauri::command]
fn start_streaming(window: tauri::Window) {
    tauri::async_runtime::spawn(async move {
        let mut model = YOLOv8::new(Args::new_from_toml(Path::new("./model_args.toml"))).unwrap();

        // define video capture objects with camera index 0
        let start = Instant::now();
        let mut caps = vec![];
        for _ in 0..=2 {
            match videoio::VideoCapture::new(0, videoio::CAP_ANY) {
                Ok(cap) => caps.push(Some(cap)),
                Err(_) => caps.push(None),
            }
        }
        let initial_camera_elapsed = start.elapsed();

        // wrap the video objects in ArcMutex to allow for shared mutable access
        let caps = Arc::new(Mutex::new(caps));

        /*
        Clone the ArcMutex to pass to the event handler
        this operation increments the reference count but
        does not clone the underlying data
        */
        let caps_clone = Arc::clone(&caps);

        let event_elapsed = Arc::new(Mutex::new(Duration::new(0, 0)));
        let event_elapsed_clone = Arc::clone(&event_elapsed);

        /*
        Define event handler to update the list of video capture objects
        when a message is received from the frontend.
        */
        let _event_handler = window.listen("update-camera", move |msg| {
            let start = Instant::now();
            let win_index;
            let cam_index;
            match msg.payload() {
                Some(msg) => {
                    let msg = msg
                        .split_whitespace()
                        .map(|s| s.parse().unwrap_or(-1))
                        .collect::<Vec<i32>>();
                    win_index = msg[0];
                    if win_index < 0 || win_index >= NUM_CAMERAS as i32 {
                        return;
                    }
                    cam_index = msg[1];
                }
                None => return,
            }
            let mut caps = caps_clone.lock().unwrap();
            match videoio::VideoCapture::new(cam_index, videoio::CAP_ANY) {
                Ok(cap) => {
                    caps[win_index as usize] = Some(cap);
                }
                Err(_) => {
                    caps[win_index as usize] = None;
                }
            }

            let mut elapsed = event_elapsed_clone.lock().unwrap();
            *elapsed = start.elapsed();
        });

        println!("Initial camera elapsed: {:?}", initial_camera_elapsed);
        println!(
            "Event handler elapsed: {:?}",
            *event_elapsed.lock().unwrap()
        );

        // define vector to store images
        let mut imgs = vec![];
        loop {
            /*
            Define vector to store the baset64 encoded images.
            If an image is invalid, an error message is pushed
            instead.
            */
            let start = Instant::now();
            let mut final_img_strs = vec!["".to_string(); NUM_CAMERAS];
            {
                // limit the scope of the lock to avoid deadlock with event_handler
                let mut caps = caps.lock().unwrap();

                // get frames from each camera
                for (i, cap) in caps[..].iter_mut().enumerate() {
                    if let Some(c) = cap {
                        if let Some(img) = get_frame_from_cap(c) {
                            // can get frame --> image is valid and push image
                            imgs.push(img);
                        } else {
                            // can't get frame --> image is invalid and push empty image
                            imgs.push(DynamicImage::default());
                            final_img_strs[i] =
                                "Error: Cannot fetch image from camera.".to_string();
                        }
                    } else {
                        // camera does not exist --> image is invalid and push empty image
                        imgs.push(DynamicImage::default());
                        final_img_strs[i] = "Error: Camera does not exist.".to_string();
                    }
                }
            }
            let get_frame_elapsed = start.elapsed();

            let start = Instant::now();
            // run inference
            let results = model.run(&imgs).unwrap();
            let inference_elapsed = start.elapsed();

            let start = Instant::now();
            // plot images
            let ploted_imgs = model.plot_batch(&results, &imgs[..], None);
            let plot_elapsed = start.elapsed();

            let start = Instant::now();
            // convert images to base64 and
            for (i, img) in ploted_imgs.iter().enumerate() {
                if final_img_strs[i].is_empty() {
                    let img_str = image_to_base64(&DynamicImage::ImageRgb8(img.clone()));
                    final_img_strs[i] = img_str;
                }
            }
            let base64_elapsed = start.elapsed();

            // print times
            println!("Get frame elapsed: {:?}", get_frame_elapsed);
            println!("Inference elapsed: {:?}", inference_elapsed);
            println!("Plot elapsed: {:?}", plot_elapsed);
            println!("Base64 elapsed: {:?}\n", base64_elapsed);

            window.emit("image-sources", final_img_strs).unwrap();
            imgs.clear();
        }
    });
}

#[tauri::command]
fn poll_and_emit_image_sources(window: tauri::Window) {
    tauri::async_runtime::spawn(async move {
        loop {
            let indices = get_camera_indices();
            println!("Available cameras: {:?}", indices);
            window
                .emit("available-cameras", indices)
                .unwrap();
            std::thread::sleep(std::time::Duration::from_secs(20));
        }
    });
}

pub fn run() {
    tauri::Builder::default()
        .setup(|_app| {
            // grab cameras and start inference
            // start_streaming();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            poll_and_emit_image_sources,
            start_streaming,
            update_camera,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
