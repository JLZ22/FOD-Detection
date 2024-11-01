use anyhow::{bail, Error, Result};
use image::{DynamicImage, ImageFormat, Rgb, RgbImage, GenericImage, GenericImageView};
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

struct Camera {
    cap: videoio::VideoCapture,
    index: i32,
}

enum CameraResult {
    Camera(Camera),
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

// eventually allow users to select aspect ratio??????
fn set_cap_properties(cap: &mut videoio::VideoCapture) {
    cap.set(videoio::CAP_PROP_FRAME_WIDTH, 640.0).unwrap();
    cap.set(videoio::CAP_PROP_FRAME_HEIGHT, 480.0).unwrap();
    cap.set(videoio::CAP_PROP_FPS, 30.0).unwrap();
}

fn setup_camera_update_listener(
    window: tauri::Window,
    tx: mpsc::SyncSender<CameraResult>,
    view: String,
) {
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

        let cap = videoio::VideoCapture::new(index, CAP_ANY);

        match cap {
            Ok(mut cap) => {
                set_cap_properties(&mut cap);

                tx.send(CameraResult::Camera(Camera { cap, index }))
                    .expect("Reciever unexpectedly hung up when sending Camera struct.");
            }
            Err(_) => {
                tx.send(CameraResult::Error(format!("Camera {} is invalid.", index)))
                    .expect("Reciever unexpectedly hung up when sending Camera struct.");
            }
        }
    });
}

/*
Intended to be run in a separate thread. Continuously captures frames from a camera and
listens to update-camera events from the frontend to change the camera index.
*/
fn setup_capture(tx: mpsc::SyncSender<Frame>, window: tauri::Window, view: String) {
    let (tx_camera_update, rx_camera_update) = mpsc::sync_channel::<CameraResult>(1);
    setup_camera_update_listener(window.clone(), tx_camera_update, view);

    let cap = videoio::VideoCapture::new(0, videoio::CAP_ANY);

    let mut cam_result = match cap {
        Ok(mut cap) => {
            set_cap_properties(&mut cap);

            CameraResult::Camera(Camera { cap, index: 0 })
        }
        Err(_) => CameraResult::Error("Camera 0 is invalid.".to_string()),
    };

    // resize takes up ~90% of processing time (500ms / 550ms)
    loop {
        match cam_result {
            CameraResult::Camera(ref mut c) => match get_frame_from_cap(&mut c.cap) {
                Ok(img) => {
                    if tx.try_send(Frame::Image(img)).is_err() {
                        thread::sleep(Duration::from_millis(10));
                    }
                }
                Err(_) => {
                    tx.send(Frame::Error(format!(
                        "Could not retrieve frame from camera {}.",
                        c.index
                    )))
                    .expect("Failed to send error message.");
                    thread::sleep(Duration::from_millis(100));
                }
            },
            CameraResult::Error(ref e) => {
                tx.send(Frame::Error(e.clone()))
                    .expect("Failed to send error message.");
                thread::sleep(Duration::from_millis(100));
            }
        }

        // check for camera update
        if let Ok(c) = rx_camera_update.try_recv() {
            cam_result = c;
        }
    }
}

pub fn setup_captures(window: tauri::Window, views: Vec<&str>) -> Vec<mpsc::Receiver<Frame>> {
    let mut recievers = vec![];
    for view in views {
        let (tx, rx) = mpsc::sync_channel::<Frame>(1);
        let w_clone = window.clone();
        let view = view.to_string().clone();
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

pub fn pad_to_size(img: DynamicImage, target_h: u32, target_w: u32, fill_value: u8) -> DynamicImage {
    let (w, h) = img.dimensions();
    
    // If the image is already the target size, return as-is
    if w == target_w && h == target_h {
        return img;
    }

    // Create a new image with the target size and fill it with the fill_value (for an RGBA image)
    let mut padded_img = RgbImage::from_pixel(target_w, target_h, Rgb([fill_value, fill_value, fill_value]));

    // Calculate the offset to center the original image
    let x_offset = (target_w - w) / 2;
    let y_offset = (target_h - h) / 2;

    // Copy the original image onto the center of the padded image
    padded_img.copy_from(&img.to_rgb8(), x_offset, y_offset).expect("Image copy failed");

    // Convert the padded image to DynamicImage for consistent return type
    DynamicImage::ImageRgb8(padded_img)
}