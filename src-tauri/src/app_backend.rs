use crate::args::Args;
use crate::model::YOLOv8;
use crate::multi_capture::{self, setup_captures};
use image::{DynamicImage, ImageFormat};
use log::info;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const NUM_CAMERAS: usize = 3;
const POLL_DURATION: Duration = Duration::from_secs(30);
const INFERENCE: bool = true;
const IMAGE_FORMAT: ImageFormat = ImageFormat::Bmp;

// Sets up the emitter thread for a view.
fn setup_emitter(rx: mpsc::Receiver<DynamicImage>, window: tauri::Window, win_index: usize) {
    // ~60ms per emission excluding waiting for the next frame
    // would only be bottleneck if we are running > 20fps
    loop {
        let img = rx
            .recv()
            .expect("Failed to recieve batch from capture thread.");
        window
            .emit(
                &format!("image-payload-{}", win_index)[..],
                multi_capture::convert_to_bytes(&img, IMAGE_FORMAT),
            )
            .expect("Failed to emit image payload.");
    }
}

// Sets up the emitter threads for each view.
fn setup_emitters(window: tauri::Window, num_cameras: i32) -> Vec<mpsc::SyncSender<DynamicImage>> {
    let mut senders = vec![];
    for i in 0..num_cameras {
        let (tx, rx) = mpsc::sync_channel::<DynamicImage>(5);
        let w_clone = window.clone();
        thread::spawn(move || setup_emitter(rx, w_clone, i as usize));
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
    let frame_recievers = setup_captures(window.clone(), NUM_CAMERAS as i32);
    // set up emitter threads
    let payload_senders = setup_emitters(window.clone(), NUM_CAMERAS as i32);

    // spawn inference thread to listen for frames, run inference, 
    // and pass results to emitter threads
    std::thread::spawn(move || {
        info!("Starting multi-camera capture and inference loop...\n");
        let mut loop_count = 0; // for periodic logging
        loop {
            let log = loop_count >= 10;

            let loop_start = Instant::now();
            let mut imgs = vec![DynamicImage::new_rgba8(0, 0); NUM_CAMERAS];
            let mut err = vec![false; NUM_CAMERAS];

            // get a Frame from reciever and update imgs/err appropriately
            let start = Instant::now();
            for (i, rx) in frame_recievers.iter().enumerate() {
                let frame = rx
                    .recv()
                    .expect("Failed to recieve frame from capture thread.");
                match frame {
                    Ok(img) => {
                        imgs[i] = img;
                    }
                    Err(_) => {
                        err[i] = true;
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
                if !err[i] {
                    tx.send(imgs[i].clone())
                        .expect("Failed to send batch to emitter thread.");
                }
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
