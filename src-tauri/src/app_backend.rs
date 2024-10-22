use crate::args::Args;
use crate::model::YOLOv8;
use crate::multi_capture::{self, setup_captures};
use image::{DynamicImage, ImageFormat};
use log::info;
use serde::Serialize;
use std::path::Path;
use std::time::{Duration, Instant};

const NUM_CAMERAS: usize = 3;
const VIEWS: [&str; NUM_CAMERAS] = ["top", "left", "front"];
const POLL_DURATION: Duration = Duration::from_secs(30);
const INFERENCE: bool = true;
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

/*
Create payloads from images and errors in parallel. This
consumes the images and errors and returns a vector of payloads.
*/
async fn construct_payloads(
    imgs: Vec<DynamicImage>,
    img_fmt: ImageFormat,
    err: Vec<String>,
) -> Vec<Payload> {
    let mut join_handles = vec![];

    for (img, err_msg) in imgs.into_iter().zip(err.into_iter()) {
        // spawn a task to convert the image to bytes
        join_handles.push(tauri::async_runtime::spawn(async move {
            let mut payload = Payload::default();
            if err_msg.is_empty() {
                payload.image = multi_capture::convert_to_bytes(&img, img_fmt);
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
    std::thread::spawn(move || loop {
        let indices = multi_capture::get_camera_indices();
        window.emit("available-cameras", indices).unwrap();
        std::thread::sleep(POLL_DURATION);
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

        // setup capture threads and recievers
        // capture threads send Frame enums to recievers
        let recievers = setup_captures(window.clone(), NUM_CAMERAS, VIEWS.to_vec());

        info!("Starting multi-camera capture and inference loop...\n");
        loop {
            info!("Starting next Iteration...");
            let loop_start = Instant::now();
            let mut imgs = vec![DynamicImage::new_rgba8(0, 0); NUM_CAMERAS];
            let mut err = vec![String::default(); NUM_CAMERAS];

            // get a Frame from reciever and update imgs/err appropriately
            let start = Instant::now();
            for (i, rx) in recievers.iter().enumerate() {
                let frame = rx.recv().expect("Failed to recieve frame from capture thread.");
                match frame {
                    multi_capture::Frame::Image(img) => {
                        imgs[i] = img;
                    }
                    multi_capture::Frame::Error(e) => {
                        err[i] = e;
                    }
                }
            }
            info!("Get frames: {:?}", start.elapsed());

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
            info!(
                "{}",
                format!("Byte conversion time: {:?}", byte_conversion_time)
            );
            info!("{}", format!("Emit time: {:?}", emit_time));
            info!(
                "{}",
                format!("Total loop time: {:?}\n", loop_start.elapsed())
            );
        }
    });
}
