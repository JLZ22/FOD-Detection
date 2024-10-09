use crate::args::Args;
use crate::model::YOLOv8;
use image::{DynamicImage, ImageFormat};
use mat2image::ToImage;
use opencv::{prelude::*, videoio};
use serde::Serialize;
use std::io::{Cursor, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const NUM_CAMERAS: usize = 3;
const POLL_DURATION: Duration = Duration::from_secs(2);
const INFERENCE: bool = true;
const LOG_OUTPUT: bool = true;

#[derive(Debug, Clone, Serialize)]
struct Payload {
    image: Vec<u8>,
    error: String,
}

#[allow(dead_code)]
impl Payload {
    fn new(image: Vec<u8>, error: String) -> Self {
        Self { image, error }
    }

    fn default() -> Self {
        Self {
            image: vec![],
            error: "".to_string(),
        }
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
fn get_frame_from_cap(cap: &mut videoio::VideoCapture) -> Option<DynamicImage> {
    let mut img = Mat::default();
    if cap.read(&mut img).unwrap_or(false) {
        match img.to_image_par() {
            Ok(image) => {
                return Some(image);
            }
            Err(_) => {
                return None;
            }
        }
    } else {
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
    logger: Arc<Mutex<std::fs::File>>,
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
        drop(caps);

        let msg = format!("Camera update handler elapsed: {:?}", start.elapsed());
        {
            log(msg, &mut logger.lock().unwrap());
        }
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

/*
Times for 3 video capture objects using Mac FaceTime HD Camera:
    Initial camera elapsed: 1.427801208s
    Event handler elapsed: 73.946833ms
    Get frame elapsed: 103.224708ms
    Inference elapsed: 2.240577042s
    Plot elapsed: 185.965083ms
    Base64 elapsed: 1.199653625s

notes:
    - emitting is slow for frequent updates

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
        // Clear output file
        let mut options = std::fs::OpenOptions::new();
        let f = Arc::new(Mutex::new(
            options
                .write(true)
                .truncate(true)
                .open("../.output")
                .expect("Failed to open output file"),
        ));

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
        {
            log(
                format!("Initial Camera Setup elapsed: {:?}\n", start.elapsed()),
                &mut f.lock().unwrap(),
            );
        }

        // wrap the video objects in ArcMutex to allow for shared mutable access
        let caps = Arc::new(Mutex::new(caps));

        // create event handler to update video capture objects
        let _event_handler =
            build_camera_update_handler(&window, Arc::clone(&caps), Arc::clone(&f));

        // define vector to store images
        {
            log(format!("Starting streaming...\n"), &mut f.lock().unwrap());
        }
        loop {
            let loop_start = Instant::now();
            /*
            Define vector to store the baset64 encoded images.
            If an image is invalid, an error message is pushed
            instead.
            */
            let start = Instant::now();
            let mut imgs = vec![DynamicImage::default(); NUM_CAMERAS];
            let mut err = vec![""; NUM_CAMERAS];
            let mut frame_times = vec![];
            {
                // limit the scope of the lock to avoid deadlock with event_handler
                let mut caps = caps.lock().unwrap();

                // get frames from each camera
                for (i, cap) in caps[..].iter_mut().enumerate() {
                    let start = Instant::now();
                    if let Some(c) = cap {
                        if let Some(img) = get_frame_from_cap(c) {
                            // can get frame --> image is valid and push image
                            imgs[i] = img;
                        } else {
                            err[i] = "Error: Cannot fetch image from camera.";
                        }
                    } else {
                        err[i] = "Error: Camera does not exist.";
                    }
                    frame_times.push(start.elapsed());
                }
            }
            {
                let mut s = format!("[Get frames]: {:?}\n", start.elapsed());
                // add the individual times
                for (i, time) in frame_times.iter().enumerate() {
                    s += &format!("\t- Time to grab frame for window {:?}: {:?}\n", i, time)[..];
                }
                s.pop(); // remove last newline for formatting
                log(s, &mut f.lock().unwrap());
            }

            if INFERENCE {
                // run inference
                let results = model.run(&imgs, &mut f.lock().unwrap()).unwrap();

                // plot images
                let start = Instant::now();
                let ploted_imgs = model.plot_batch(&results, &imgs[..]);
                log(
                    format!("[Plot results]: {:?}", start.elapsed()),
                    &mut f.lock().unwrap(),
                );

                imgs = ploted_imgs
                    .iter()
                    .map(|img| DynamicImage::ImageRgb8(img.clone()))
                    .collect();
            }

            // convert images to bytes
            // TODO: use multiple threads
            let mut emit_times = vec![];
            let mut to_bytes_time = vec![];
            let img_fmt = ImageFormat::Bmp;
            for (i, img) in imgs.iter().enumerate() {
                let mut payload = Payload::default();
                let error_msg = err[i];

                let start = Instant::now();
                if error_msg.len() == 0 {
                    payload.image = convert_to_bytes(img, img_fmt);
                } else {
                    payload.error = error_msg.to_string();
                }
                to_bytes_time.push(start.elapsed());

                let tag = &format!("image-payload-{i}")[..];
                let start = Instant::now();
                window.emit(tag, payload).unwrap();
                emit_times.push(start.elapsed());
            }

            /*
            Log the times for emitting images, converting images to bytes,
            and the total time for the loop.
            */
            {
                let total_emit = emit_times.iter().sum::<Duration>();
                let total_to_bytes = to_bytes_time.iter().sum::<Duration>();
                let mut emit_time_string = format!("[Emit images]: {:?}\n", total_emit);
                let mut byte_time_string = format!(
                    "[Convert images to bytes {:?}]: {:?}\n", img_fmt,
                    total_to_bytes
                );
                let total_time_string = 
                    format!("- - - - - - - - - -\n[Total loop time]: {:?}\n", loop_start.elapsed());

                for i in 0..NUM_CAMERAS {
                    emit_time_string += &format!(
                        "\t- Time to emit image for window {:?}: {:?}\n",
                        i, emit_times[i]
                    )[..];
                    byte_time_string += &format!(
                        "\t- Time to convert image to bytes for window {:?}: {:?}\n",
                        i, to_bytes_time[i]
                    )[..];
                }

                // remove last newline for formatting
                emit_time_string.pop();
                byte_time_string.pop();

                let mut file = f.lock().unwrap();
                log(emit_time_string, &mut file);
                log(byte_time_string, &mut file);
                log(total_time_string, &mut file);
            }
        }
    });
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
