use anyhow::{bail, Error};
use image::{imageops, DynamicImage, ImageFormat};
use log::info;
use mat2image::ToImage;
use opencv::videoio::CAP_ANY;
use opencv::{prelude::*, videoio};
use std::io::Cursor;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const NUM_CAMERAS: usize = 3;
const VIEWS: [&str; NUM_CAMERAS] = ["top", "left", "front"];

pub fn get_camera_indices() -> Vec<i32> {
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

/*
Intended to be run in a separate thread. Continuously captures frames from a camera and
listens to update-camera events from the frontend to change the camera index.
*/
fn setup_capture(win_index: usize, tx: mpsc::SyncSender<DynamicImage>, window: tauri::Window) {
    let (tx_camera_update, rx_camera_update) =
        mpsc::sync_channel::<Result<videoio::VideoCapture, _>>(1);
    window.listen(format!("update-camera-{}", VIEWS[win_index]), move |msg| {
        // decode the payload
        let index = match msg.payload() {
            Some(msg) => {
                let msg = msg
                    .split_whitespace()
                    .map(|s| s.parse().unwrap_or(-1))
                    .collect::<Vec<i32>>();

                msg[0]
            }
            None => -1,
        };

        // do nothing if the index is invalid (an error occurred with sending the payload
        if index < 0 {
            // potentially emit an error to the frontend
            return;
        }

        // create a new VideoCapture object and send it to the main thread
        let cap = videoio::VideoCapture::new(index, CAP_ANY);
        tx_camera_update
            .send(cap)
            .expect("Reciever unexpectedly hung up when sending VideoCapture object.");
    });

    let mut cap = videoio::VideoCapture::new(0, CAP_ANY);
    loop {
        if let Ok(c) = cap.as_mut() {
            match get_frame_from_cap(c) {
                Ok(img) => {
                    // ~400 ms for 4032x3024 -> 300x225
                    let img = img.resize(640, 640, imageops::FilterType::Triangle);

                    tx.send(img).unwrap();
                }
                Err(e) => {
                    thread::sleep(Duration::from_millis(100));
                    // emit error to frontend
                    // do nothing and listen for camera change event
                    // update the camera and continue the loop
                }
            }
        } else {
            thread::sleep(Duration::from_millis(100));
            // emit error to frontend
            // do nothing and listen for camera change event
            // update the camera and continue the loop
        }

        // check for camera update
        if let Ok(c) = rx_camera_update.try_recv() {
            cap = c;
        }
    }
}

pub fn setup_captures(window: tauri::Window) -> Vec<mpsc::Receiver<DynamicImage>> {
    let mut recievers = vec![];
    for i in 0..NUM_CAMERAS {
        let (tx, rx) = mpsc::sync_channel::<DynamicImage>(1);
        let w_clone = window.clone();
        thread::spawn(move || setup_capture(i, tx, w_clone));
        recievers.push(rx);
    }

    recievers
}

pub fn get_imgs(
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

pub fn convert_to_bytes(img: &DynamicImage, format: ImageFormat) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut writer = std::io::BufWriter::new(Cursor::new(&mut buf));

    img.write_to(&mut writer, format).unwrap();
    drop(writer); // drop to flush the writer and ensure all data is written

    buf
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
pub fn build_camera_update_handler(
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
