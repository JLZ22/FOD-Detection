use anyhow::{bail, Error, Result};
use image::{DynamicImage, GenericImage, GenericImageView, ImageFormat, Rgb, RgbImage};
use mat2image::ToImage;
use opencv::videoio::CAP_ANY;
use opencv::{prelude::*, videoio};
use std::io::Cursor;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

struct Camera {
    cap: videoio::VideoCapture,
    index: i32,
}

pub fn get_camera_indices() -> Vec<i32> {
    let mut indices = vec![];
    for i in 0..8 {
        let mut cap = match videoio::VideoCapture::new(i, videoio::CAP_ANY) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if cap.is_opened().is_ok_and(|opened| opened) {
            indices.push(i);
            cap.release().unwrap();
        }
    }
    indices
}

// Get a frame from a video capture object and convert it to a DynamicImage
fn get_frame_from_cap(cam: &mut Camera) -> Result<DynamicImage, Error> {
    let mut img = Mat::default();
    let cap = &mut cam.cap;
    if cap.read(&mut img).unwrap_or(false) {
        match img.to_image_par() {
            Ok(image) => Ok(image),
            Err(_) => {
                bail!("Error: Could not convert Mat to DynamicImage.");
            }
        }
    } else {
        bail!("Error: Could not read frame from camera {}. \nTip: Check camera connection.", cam.index);
    }
}

// eventually allow users to select aspect ratio??????
fn set_cap_properties(cap: &mut videoio::VideoCapture) {
    cap.set(videoio::CAP_PROP_FRAME_WIDTH, 640.0).unwrap();
    cap.set(videoio::CAP_PROP_FRAME_HEIGHT, 480.0).unwrap();
    cap.set(videoio::CAP_PROP_FPS, 30.0).unwrap();
}

fn setup_camera_update_listener(
    window: tauri::Window,
    tx: mpsc::SyncSender<Result<Camera, ()>>,
    win_id: i32,
) {
    let win_clone = window.clone();

    window.listen(format!("update-camera-{}", win_id), move |msg| {
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

        // check if there was an issue with the payload (index is -1)
        if index == -1 {
            // emit error message to the frontend
            win_clone
                .emit(
                    &format!("error-{}", win_id),
                    "Error: invalid or non-existant payload.",
                )
                .expect("Failed to emit error message.");

            // send error message to the capture thread
            tx.send(Err(()))
                .expect("Reciever unexpectedly hung up when sending Err.");
            return;
        }

        let cap = videoio::VideoCapture::new(index, CAP_ANY);

        match cap {
            Ok(mut cap) => {
                set_cap_properties(&mut cap);

                tx.send(Ok(Camera { cap, index }))
                    .expect("Reciever unexpectedly hung up when sending Camera struct.");
            }
            Err(_) => {
                // emit error message to the frontend
                win_clone
                    .emit(
                        &format!("error-{}", win_id),
                        &format!("Error: Camera {} is invalid.", index),
                    )
                    .expect("Failed to emit error message.");

                // send error message to the capture thread
                tx.send(Err(()))
                    .expect("Reciever unexpectedly hung up when sending Err.");
            }
        }
    });
}

/*
Continuously captures frames from a camera and listens to 
update-camera events from the frontend to change the camera index.
*/
fn setup_capture(
    tx: mpsc::SyncSender<Result<DynamicImage, ()>>,
    window: tauri::Window,
    win_id: i32,
) {
    let (tx_camera_update, rx_camera_update) = mpsc::sync_channel::<Result<Camera, ()>>(1);
    setup_camera_update_listener(window.clone(), tx_camera_update, win_id);

    // initialize the camera to error state, allowing 
    // it to be updated in the following loop
    let mut cam = Err(());

    loop {
        // check if the camera is valid
        match cam {
            Ok(ref mut c) => 
                // check if the frame retrieval was successful
                match get_frame_from_cap(c) {
                    Ok(img) => {
                        // send to inference thread if it is ready to recieve
                        // otherwise, discard the frame
                        if tx.try_send(Ok(img)).is_err() {
                            thread::sleep(Duration::from_millis(10));
                        }
                    }
                    Err(e) => {
                        // emit the frame retrieval error to the frontend
                        window
                            .emit(&format!("error-{}", win_id), &e.to_string())
                            .expect("Failed to emit error message.");

                        // send empty error to the inference thread
                        tx.send(Err(())).expect("Failed to send error message.");
                        thread::sleep(Duration::from_millis(50));
                    }
            },
            // Do nothing if the camera is invalid. Error has already been emitted.
            _ => {
                thread::sleep(Duration::from_millis(50));
            }
        }

        // check for camera update
        if let Ok(c) = rx_camera_update.try_recv() {
            cam = c;
        }
    }
}


// Set up capture threads for each camera and return a vector of recievers
pub fn setup_captures(
    window: tauri::Window,
    num_cameras: i32,
) -> Vec<mpsc::Receiver<Result<DynamicImage, ()>>> {
    let mut recievers = vec![];
    for i in 0..num_cameras {
        let (tx, rx) = mpsc::sync_channel::<Result<DynamicImage, ()>>(1);
        let w_clone = window.clone();
        thread::spawn(move || setup_capture(tx, w_clone, i));
        recievers.push(rx);
    }

    recievers
}

pub fn convert_to_bytes(img: &DynamicImage, format: ImageFormat) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut writer = std::io::BufWriter::new(Cursor::new(&mut buf));

    // Write the image to the buffer
    img.write_to(&mut writer, format).unwrap();
    drop(writer); // drop to flush the writer and ensure all data is written

    buf
}

pub fn pad_to_size(
    img: DynamicImage,
    target_h: u32,
    target_w: u32,
    fill_value: u8,
) -> DynamicImage {
    let (w, h) = img.dimensions();

    // If the image is already the target size, return as-is
    if w == target_w && h == target_h {
        return img;
    }

    // Create a new image with the target size and fill it with the fill_value (for an RGBA image)
    let mut padded_img = RgbImage::from_pixel(
        target_w,
        target_h,
        Rgb([fill_value, fill_value, fill_value]),
    );

    // Copy the original image onto the padded image
    padded_img
        .copy_from(&img.to_rgb8(), 0, 0)
        .expect("Image copy failed");

    // Convert the padded image to DynamicImage for consistent return type
    DynamicImage::ImageRgb8(padded_img)
}
