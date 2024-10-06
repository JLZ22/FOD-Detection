use crate::model::YOLOv8;
use crate::args::Args;
use std::io::Cursor;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use image::{DynamicImage, ImageFormat};
use mat2image::ToImage;
use opencv::{prelude::*, videoio};

const NUM_CAMERAS: usize = 3;
const INFERENCE: bool = true;

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
    let start = Instant::now();
    let mut img = Mat::default();
    if cap.read(&mut img).unwrap_or(false) {
        match img.to_image_par() {
            Ok(image) => {
                println!("get_frame_from_cap: {:?}", start.elapsed());
                return Some(image);
            }
            Err(_) => {
                println!("get_frame_from_cap: {:?}", start.elapsed());
                return None;
            }
        }
    } else {
        println!("get_frame_from_cap: {:?}", start.elapsed());
        return None;
    }
}

#[tauri::command]
pub fn update_camera(window: tauri::Window, win_index: i32, cam_index: i32) {
    window.trigger(
        "update-camera",
        Some(format!("{win_index} {cam_index}").to_string()),
    );
}

/*
Create an event handler that listens for update-camera messages from the
frontend and updates the list of video capture objects accordingly.
*/
fn build_camera_update_handler(
    window: &tauri::Window,
    caps_clone: Arc<Mutex<Vec<Option<videoio::VideoCapture>>>>,
) -> tauri::EventHandler {
    window.listen("update-camera", move |msg| {
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

        println!("Event handler elapsed: {:?}", start.elapsed());
    })
}

/*
Times for 3 video capture objects using Mac FaceTime HD Camera:
    Initial camera elapsed: 1.427801208s
    Event handler elapsed: 73.946833ms
    Get frame elapsed: 103.224708ms
    Inference elapsed: 2.240577042s
    Plot elapsed: 185.965083ms
    Base64 elapsed: 1.199653625s

TODO: send each image as a struct {processed_image in bytes, error_message}. emit per window
TODO: grab images with multiple threads
TODO: send raw bytes instead of base64 encoding
TODO: read images on both front and backend and only send the bounding box info
*/
#[tauri::command]
pub fn start_streaming(window: tauri::Window) {
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
        println!("Initial camera elapsed: {:?}", start.elapsed());

        // wrap the video objects in ArcMutex to allow for shared mutable access
        let caps = Arc::new(Mutex::new(caps));

        // create event handler to update video capture objects
        let _event_handler = build_camera_update_handler(&window, Arc::clone(&caps));

        // define vector to store images
        loop {
            let mut imgs = vec![];
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

            if !INFERENCE {
                let start = Instant::now();
                for (i, img) in imgs.iter().enumerate() {
                    if final_img_strs[i].is_empty() {
                        final_img_strs[i] = image_to_base64(&img);
                    }
                }
                let base64_elapsed = start.elapsed();

                println!("Get frame elapsed: {:?}", get_frame_elapsed);
                println!("Base64 elapsed: {:?}\n", base64_elapsed);

                window.emit("image-sources", final_img_strs).unwrap();

                continue;
            }

            // run inference
            let results = model.run(&imgs).unwrap();

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
            println!("Plot elapsed: {:?}", plot_elapsed);
            println!("Base64 elapsed: {:?}\n", base64_elapsed);

            window.emit("image-sources", final_img_strs).unwrap();
        }
    });
}

#[tauri::command]
pub fn poll_and_emit_image_sources(window: tauri::Window) {
    tauri::async_runtime::spawn(async move {
        loop {
            let indices = get_camera_indices();
            println!("Available cameras: {:?}", indices);
            window.emit("available-cameras", indices).unwrap();
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    });
}