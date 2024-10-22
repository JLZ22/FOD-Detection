# FOD-Detection

## Notes for cloning/setup

### Submodule

This project contains a submodule. 

Git >=2.13: 
```bash
git clone --recurse-submodules https://github.com/JLZ22/FOD-Detection
```

Git  <2.13:  
```bash
git clone --recursive https://github.com/JLZ22/FOD-Detection
```

If you have already cloned the repository:
```bash
cd ./FOD-Detection/src-tauri/mat2image
git submodule update --init --recursive
```

### Model Args

To specify the parameters you would like for the YOLO model that you are loading using ONNX, you can create a `src-tauri/model_args.toml` file which can contain any of the following fields. 

```toml
model = "./models/yolov8n.onnx"
source = ""
device_id = 0
trt = false
cuda = true
batch = 3
batch_min = 1
batch_max = 3
fp16 = false
task = "detect"
nc = 5
nk = 0
nm = 0
width = 512
height = 512
conf = 0.5
iou = 0.5
kconf = 0.5
plot = false
profile = false
```

## Usage

Once you've cloned the project and installed dependencies with `yarn install`:
```bash
yarn tauri dev
```

To create a production version of the app:
```bash
yarn tauri build
```

You can preview the production build with `yarn run preview`.

## TODO: Write other sections
