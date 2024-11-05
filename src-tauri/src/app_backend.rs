use crate::args::Args;
use crate::model::YOLOv8;
use crate::multi_capture::{self, setup_captures};
use image::{DynamicImage, ImageFormat};
use log::info;
use serde::Serialize;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const NUM_CAMERAS: usize = 3;
const VIEWS: [&str; NUM_CAMERAS] = ["top", "left", "front"];
const POLL_DURATION: Duration = Duration::from_secs(30);
const INFERENCE: bool = true;
const IMAGE_FORMAT: ImageFormat = ImageFormat::Bmp;

struct Batch {
    image: DynamicImage,
    error: String,
}

// This could be an enum but it is ~5-10ms slower
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

impl Payload {
    fn new(image: Vec<u8>, error: String) -> Self {
        Self { image, error }
    }
}

// Sets up the emitter thread for a view.
fn setup_emitter(rx: mpsc::Receiver<Batch>, window: tauri::Window, win_index: usize) {
    // ~60ms per emission excluding waiting for the next frame
    // would only be bottleneck if we are running > 20fps
    loop {
        let batch = rx
            .recv()
            .expect("Failed to recieve batch from capture thread.");
        window
            .emit(
                &format!("image-payload-{}", win_index)[..],
                Payload::new(
                    multi_capture::convert_to_bytes(&batch.image, IMAGE_FORMAT),
                    batch.error,
                ),
            )
            .expect("Failed to emit image payload.");
    }
}

// Sets up the emitter threads for each view.
fn setup_emitters(window: tauri::Window, views: Vec<&str>) -> Vec<mpsc::SyncSender<Batch>> {
    let mut senders = vec![];
    for (i, _) in views.iter().enumerate() {
        let (tx, rx) = mpsc::sync_channel::<Batch>(5);
        let w_clone = window.clone();
        thread::spawn(move || setup_emitter(rx, w_clone, i));
        senders.push(tx);
    }

    senders
}

// Polls for available camera sources and emits the indices to the frontend.
#[tauri::command]
pub fn poll_and_emit_image_sources(window: tauri::Window) {
    std::thread::spawn(move || loop {
        let indices = multi_capture::get_camera_indices();
        window.emit("available-cameras", indices).unwrap();
        std::thread::sleep(POLL_DURATION);
    });
}

/*
Starts the streaming process by setting up the capture threads, model thread,
and emitter threads. The capture threads grab the frames from the camera and
and send them to the model thread through channels. The model thread runs the
batched inference on the frames, plots the results, and sends each frame to
their respective emitter threads. The emitter threads convert the frames to
bytes and send them to the frontend through the window.
*/
#[tauri::command]
pub fn start_streaming(window: tauri::Window) {
    info!("Starting streaming...");

    let mut model = YOLOv8::new(Args::new_from_toml(Path::new("./model_args.toml"))).unwrap();

    // setup capture threads
    let frame_recievers = setup_captures(window.clone(), VIEWS.to_vec());
    // set up emitter threads
    let payload_senders = setup_emitters(window.clone(), VIEWS.to_vec());

    std::thread::spawn(move || {
        info!("Starting multi-camera capture and inference loop...\n");
        let mut loop_count = 0; // for periodic logging
        loop {
            let log = loop_count >= 10;
            if log {
                info!("Starting next Iteration...");
            }

            let loop_start = Instant::now();
            let mut imgs = vec![DynamicImage::new_rgba8(0, 0); NUM_CAMERAS];
            let mut err = vec![String::default(); NUM_CAMERAS];

            // get a Frame from reciever and update imgs/err appropriately
            let start = Instant::now();
            for (i, rx) in frame_recievers.iter().enumerate() {
                let frame = rx
                    .recv()
                    .expect("Failed to recieve frame from capture thread.");
                match frame {
                    multi_capture::Frame::Image(img) => {
                        imgs[i] = img;
                    }
                    multi_capture::Frame::Error(e) => {
                        err[i] = e;
                    }
                }
            }
            if log {
                info!("Get frames: {:?}", start.elapsed());
            }

            if INFERENCE {
                // run inference
                let results = model.run(&imgs, log).expect("valid YOLOResult");

                // plot images
                let ploted_imgs = model.plot_batch(&results, &imgs[..], log);

                imgs = ploted_imgs
                    .iter()
                    .map(|img| DynamicImage::ImageRgb8(img.clone()))
                    .collect();
            }

            for (i, tx) in payload_senders.iter().enumerate() {
                tx.send(Batch {
                    image: imgs[i].clone(),
                    error: err[i].clone(),
                })
                .expect("Failed to send batch to emitter thread.");
            }

            if log {
                info!(
                    "{}",
                    format!("Total loop time: {:?}\n", loop_start.elapsed())
                );
                loop_count = 0;
            } else {
                loop_count += 1;
            }
        }
    });
}
