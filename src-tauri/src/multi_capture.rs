use tokio::sync::mpsc;
use image::{DynamicImage, ImageFormat};
use std::thread; 
use std::sync::{Arc, Mutex};
use opencv::{prelude::*, videoio};
use anyhow::{bail, Error};
use mat2image::ToImage;
use std::time::{Duration, Instant};
use std::io::Cursor;
use log::info;

const NUM_CAMERAS: usize = 3;

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

fn setup_capture(win_index: i32, tx: mpsc::Sender<DynamicImage>) {
    let mut cap = videoio::VideoCapture::new(0, videoio::CAP_ANY); 

    loop {
        
    }
}

pub fn setup_captures() -> Vec<mpsc::Receiver<DynamicImage>> {
    let (tx, top_rx) = mpsc::channel::<DynamicImage>(10);
    thread::spawn(|| setup_capture(0, tx));
    let (tx, front_rx) = mpsc::channel::<DynamicImage>(10);
    thread::spawn(|| setup_capture(1, tx));
    let (tx, left_rx) = mpsc::channel::<DynamicImage>(10);
    thread::spawn(|| setup_capture(2, tx));

    vec![top_rx, left_rx, front_rx]
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