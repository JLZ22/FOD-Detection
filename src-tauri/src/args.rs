use std::path::Path;

use figment::{
    providers::{self, Format},
    Figment,
};

use crate::YOLOTask;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default)]
pub struct Args {
    /// ONNX model path
    pub model: String,

    /// input path
    pub source: String,

    /// device id
    pub device_id: u32,

    /// using TensorRT EP
    pub trt: bool,

    /// using CUDA EP
    pub cuda: bool,

    /// input batch size
    pub batch: u32,

    /// trt input min_batch size
    pub batch_min: u32,

    /// trt input max_batch size
    pub batch_max: u32,

    /// using TensorRT --fp16
    pub fp16: bool,

    /// specify YOLO task
    pub task: Option<YOLOTask>,

    /// num_classes
    pub nc: Option<u32>,

    /// num_keypoints
    pub nk: Option<u32>,

    /// num_masks
    pub nm: Option<u32>,

    /// input image width
    pub width: Option<u32>,

    /// input image height
    pub height: Option<u32>,

    /// confidence threshold
    pub conf: f32,

    /// iou threshold in NMS
    pub iou: f32,

    /// confidence threshold of keypoint
    pub kconf: f32,

    /// plot inference result and save
    pub plot: bool,

    /// check time consumed in each stage
    pub profile: bool,
}

impl Default for Args {
    fn default() -> Self {
        Args {
            model: "./models/yolov8n.onnx".to_string(), // ONNX model path
            source: "".to_string(),                     // Input path
            device_id: 0,                               // device id
            trt: false,                                 // Enable if using TensorRT
            cuda: true,                                 // Enable if using CUDA
            batch: 3,                                   // Set the batch size to 3
            batch_min: 1,                               // If using TensorRT, min_batch size
            batch_max: 3,                               // If using TensorRT, max_batch size
            fp16: false, // Enable if you want to use FP16 precision with TensorRT
            task: Some(YOLOTask::Detect), // Define the task
            nc: Some(5), // Number of classes
            nk: None,    // Number of keypoints
            nm: None,    // Number of masks
            width: Some(512), // Input image width for YOLO model
            height: Some(512), // Input image height for YOLO model
            conf: 0.5,   // Confidence threshold for detections
            iou: 0.5,    // IoU threshold for Non-Max Suppression
            kconf: 0.5,  // Keypoint confidence threshold (if keypoints are used)
            plot: false, // Enable plotting results
            profile: false, // Enable profiling if needed
        }
    }
}

impl Args {
    pub fn new_from_toml(toml: &Path) -> Self {
        Figment::new()
            .merge(providers::Toml::file(toml))
            .extract()
            .expect("to be valid")
    }
}
