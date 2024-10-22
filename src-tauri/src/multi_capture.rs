use anyhow::{bail, Error};
use image::{imageops, DynamicImage, ImageFormat};
use mat2image::ToImage;
use opencv::videoio::CAP_ANY;
use opencv::{prelude::*, videoio};
use std::io::Cursor;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum Frame {
    Image(DynamicImage),
    Error(String),
}

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
fn setup_capture(tx: mpsc::SyncSender<Frame>, window: tauri::Window, view: String) {
    let (tx_camera_update, rx_camera_update) =
        mpsc::sync_channel::<(Result<videoio::VideoCapture, _>, i32)>(1);
    window.listen(format!("update-camera-{}", view), move |msg| {
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

        // create a new VideoCapture object and send it to the main thread
        let cap = videoio::VideoCapture::new(index, CAP_ANY);
        tx_camera_update
            .send((cap, index))
            .expect("Reciever unexpectedly hung up when sending VideoCapture object.");
    });

    let mut cap_index = 0;
    let mut cap = videoio::VideoCapture::new(cap_index, CAP_ANY);
    loop {
        if let Ok(c) = cap.as_mut() {
            match get_frame_from_cap(c) {
                Ok(img) => {
                    // ~400 ms for 4032x3024 -> 300x225
                    let img = img.resize(640, 640, imageops::FilterType::Triangle);

                    tx.send(Frame::Image(img)).unwrap();
                }
                Err(_) => {
                    tx.send(Frame::Error(format!(
                        "Could not retrieve frame from camera {}.",
                        cap_index
                    )))
                    .expect("Failed to send error message.");
                    thread::sleep(Duration::from_millis(100));
                }
            }
        } else {
            tx.send(Frame::Error(format!("Camera {} is invalid.", cap_index)))
                .expect("Failed to send error message.");
            thread::sleep(Duration::from_millis(100));
        }

        // check for camera update
        if let Ok((new_cap, new_index)) = rx_camera_update.try_recv() {
            cap = new_cap;
            cap_index = new_index;
        }
    }
}

pub fn setup_captures(
    window: tauri::Window,
    num_captures: usize,
    views: Vec<&str>,
) -> Vec<mpsc::Receiver<Frame>> {
    let mut recievers = vec![];
    for i in 0..num_captures {
        let (tx, rx) = mpsc::sync_channel::<Frame>(1);
        let w_clone = window.clone();
        let view = views[i].to_string().clone();
        thread::spawn(move || setup_capture(tx, w_clone, view));
        recievers.push(rx);
    }

    recievers
}

pub fn convert_to_bytes(img: &DynamicImage, format: ImageFormat) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut writer = std::io::BufWriter::new(Cursor::new(&mut buf));

    img.write_to(&mut writer, format).unwrap();
    drop(writer); // drop to flush the writer and ensure all data is written

    buf
}
