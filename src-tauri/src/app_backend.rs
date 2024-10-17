use crate::args::Args;
use crate::model::YOLOv8;
use anyhow::{bail, Error};
use image::{DynamicImage, ImageFormat};
use log::info;
use mat2image::ToImage;
use opencv::{prelude::*, videoio};
use serde::Serialize;
use std::io::{Cursor, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const NUM_CAMERAS: usize = 3;
const POLL_DURATION: Duration = Duration::from_secs(30);
const INFERENCE: bool = true;
const LOG_OUTPUT: bool = true;
const IMAGE_FORMAT: ImageFormat = ImageFormat::Bmp;

#[derive(Debug, Clone, Serialize)]
struct Payload {
    image: Vec<u8>,
    error: String,
}

impl Default for Payload {
    fn default() -> Self {
        Self {
            image: vec![],
            error: "".to_string(),
        }
    }
}

#[allow(dead_code)]
impl Payload {
    fn new(image: Vec<u8>, error: String) -> Self {
        Self { image, error }
    }
}

// Binary search for the maximum camera index that is available
// l should always be 0
// ~ 200-250 ms
fn get_camera_indices() -> Vec<i32> {
    let mut indices = vec![];
    for i in 0..5 {
        let mut cap = videoio::VideoCapture::new(i, videoio::CAP_ANY).unwrap();
        if cap.is_opened().unwrap() {
            indices.push(i);
            cap.release().unwrap();
        }
    }
    indices
}

// Get a frame from a video capture object and convert it to a DynamicImage
fn get_frame_from_cap(cap: &mut videoio::VideoCapture) -> Result<DynamicImage, Error> {
    let mut img = Mat::default();
    if cap.read(&mut img).unwrap_or(false) {
        match img.to_image_par() {
            Ok(image) => Ok(image),
            Err(_) => {
                bail!("Error: Cannot convert Mat to DynamicImage.");
            }
        }
    } else {
        bail!("Error: Cannot read frame from camera.");
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

        // parse the message for the window index and camera index
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

        // update the video capture object at the specified index
        let mut caps = caps_clone.lock().unwrap();
        match videoio::VideoCapture::new(cam_index, videoio::CAP_ANY) {
            Ok(cap) => {
                caps[win_index as usize] = Some(cap);
            }
            Err(_) => {
                caps[win_index as usize] = None;
            }
        }
        drop(caps);

        info!(
            "{}",
            format!("Camera update handler elapsed: {:?}", start.elapsed())
        );
    })
}

fn convert_to_bytes(img: &DynamicImage, format: ImageFormat) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut writer = std::io::BufWriter::new(Cursor::new(&mut buf));

    img.write_to(&mut writer, format).unwrap();
    drop(writer); // drop to flush the writer and ensure all data is written

    buf
}

// f is the output file and must be opened in append mode
pub fn log(msg: String, f: &mut std::fs::File) {
    if !LOG_OUTPUT {
        return;
    }
    f.write(msg.as_bytes()).unwrap();
    f.write(b"\n").unwrap();
}

fn get_imgs(
    imgs: &mut Vec<DynamicImage>,
    err: &mut Vec<String>,
    caps: &mut Vec<Option<videoio::VideoCapture>>,
    frame_times: &mut Vec<Duration>,
) {
    // get frames from each camera
    for (i, cap) in caps[..].iter_mut().enumerate() {
        let start = Instant::now();
        if let Some(c) = cap {
            match get_frame_from_cap(c) {
                Ok(img) => {
                    // can get frame --> image is valid and push image
                    imgs[i] = img;
                }
                Err(e) => {
                    err[i] = e.to_string();
                }
            }
        } else {
            err[i] = "Error: Camera does not exist.".to_string();
        }
        frame_times.push(start.elapsed());
    }
}

/*
Create payloads from images and errors in parallel. This 
consumes the images and errors and returns a vector of payloads.
*/
async fn construct_payloads(imgs: Vec<DynamicImage>, 
    img_fmt: ImageFormat, err: Vec<String>) -> Vec<Payload> {
    let mut join_handles = vec![];

    for (img, err_msg) in imgs.into_iter().zip(err.into_iter()) {
        // spawn a task to convert the image to bytes
        join_handles.push(tauri::async_runtime::spawn(async move {
            let mut payload = Payload::default();
            if err_msg.is_empty() {
                payload.image = convert_to_bytes(&img, img_fmt);
            } else {
                payload.error = err_msg;
            }

            payload
        }));
    }


    // wait for tasks to finish and return the collected payloads
    futures::future::join_all(join_handles)
        .await
        .into_iter()
        .map(|result| {
            result.unwrap_or_else(|_| {
                Payload::new(
                    Vec::new(),
                    "Error: Payload construction failure.".to_string(),
                )
            })
        })
        .collect()
}

// emit payloads to the frontend in parallel
async fn emit_payloads_parallel(window: &tauri::Window, payloads: Vec<Payload>) {
    let mut join_handles = vec![];
    for (i, payload) in payloads.into_iter().enumerate() {
        let win_clone = window.clone();
        join_handles.push(tauri::async_runtime::spawn(async move {
            let tag = &format!("image-payload-{}", i)[..];
            win_clone
                .emit(tag, payload)
                .expect("Failed to emit image payload.");
        }));
    }
    futures::future::join_all(join_handles).await;
}

#[tauri::command]
pub fn poll_and_emit_image_sources(window: tauri::Window) {
    tauri::async_runtime::spawn(async move {
        loop {
            let indices = get_camera_indices();
            window.emit("available-cameras", indices).unwrap();
            std::thread::sleep(POLL_DURATION);
        }
    });
}

/*
Times for 3 video capture objects using Mac FaceTime HD Camera:
02:45:52 [INFO] Get frames: 119.303125ms
    - Time to grab frame for window 0: 40.962166ms
    - Time to grab frame for window 1: 34.160209ms
    - Time to grab frame for window 2: 44.108042ms
02:45:55 [INFO] Model inference duration: 2.223577625s (Pre: 2.087768208s, Run: 85.704375ms, Post: 50.105042ms)
02:45:55 [INFO] plot_batch duration: 182.401583ms
02:45:55 [INFO] Byte conversion time: 62.006292ms
02:45:55 [INFO] Emit time: 516.389292ms
02:45:55 [INFO] Total loop time: 3.106455875s

^ down from 4.5s

notes:
    - emitting is slow for frequent updates

TODO: change payload to an enum 
TODO: create a camera struct that stores video cap object and win index 
TODO: create threads to infinitely read from cameras and send through a channel (acts like a buffer)
    - read from channels in the main loop
TODO: main loop performs batched inference and sends to three threads that build payload and emit

TODO: play with changing resolution and requesting a mjpeg
TODO: explore reading on a separate thread
TODO: pull frames from the cameras in parallel
TODO: look into lossy compression
TODO: increase the batch size and pull more frames per camera
    - adjust other operations accordingly (consider multi-threading)
TODO: look into migrating to web sockets for faster communication

IDEA: read images on both front and backend and only send the bounding box info
    - look into keeping count of frames on both front and back end
    - skip frames if the backend is lagging behind
*/
#[tauri::command]
pub fn start_streaming(window: tauri::Window) {
    tauri::async_runtime::spawn(async move {
        info!("Starting streaming...");

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
        info!(
            "Initial camera setup complete! Duration: {:?}",
            start.elapsed()
        );

        // wrap the video objects in ArcMutex to allow for shared mutable access
        let caps = Arc::new(Mutex::new(caps));

        // create event handler to update video capture objects
        let _event_handler = build_camera_update_handler(&window, Arc::clone(&caps));

        info!("Starting multi-camera capture and inference loop...\n");
        loop {
            info!("Starting next Iteration...");
            let loop_start = Instant::now();
            let start = Instant::now();
            let mut imgs = vec![DynamicImage::default(); NUM_CAMERAS];
            let mut err = vec![String::default(); NUM_CAMERAS];
            let mut frame_times = vec![];

            get_imgs(
                &mut imgs,
                &mut err,
                &mut caps.lock().unwrap(),
                &mut frame_times,
            );

            // log the time to get frames
            let mut s = format!("Get frames: {:?}\n", start.elapsed());
            // add the individual times
            for (i, time) in frame_times.iter().enumerate() {
                s += &format!("\t- Time to grab frame for window {:?}: {:?}\n", i, time)[..];
            }
            s.pop();
            info!("{}", s);

            if INFERENCE {
                // run inference
                let results = model.run(&imgs).unwrap();

                // plot images
                let ploted_imgs = model.plot_batch(&results, &imgs[..]); // TODO: implement in parallel

                imgs = ploted_imgs
                    .iter()
                    .map(|img| DynamicImage::ImageRgb8(img.clone()))
                    .collect();
            }

            // convert images to payloads
            let start = Instant::now();
            let payloads = construct_payloads(imgs, IMAGE_FORMAT, err).await;
            let byte_conversion_time = start.elapsed();

            // emit payloads
            let start = Instant::now();
            emit_payloads_parallel(&window, payloads).await;
            let emit_time = start.elapsed();

            // log the times
            info!("{}", format!("Byte conversion time: {:?}", byte_conversion_time));
            info!("{}", format!("Emit time: {:?}", emit_time));
            info!("{}", format!("Total loop time: {:?}\n", loop_start.elapsed()));
        }
    });
}
