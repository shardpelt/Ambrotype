# Using your own AI models

The Subject, Background and People masks come from AI models that run on your computer. Settings → AI models lists the ones Ambrotype can install for you. You can also add any other ONNX model, for example one the list can't include.

## MediaPipe's people model (hair, skin, clothes)

Google's MediaPipe publishes a model that finds people and splits them into hair, face skin, body skin and clothes (Apache-2.0, [model card](https://storage.googleapis.com/mediapipe-assets/Model%20Card%20Multiclass%20Segmentation.pdf)). It comes as a TensorFlow Lite file, which Ambrotype can't run, so convert it to ONNX once. It takes a few minutes.

1. **Install the converter** (Python 3, about 600 MB with TensorFlow):
   ```sh
   python3 -m venv convert
   convert/bin/pip install tf2onnx tensorflow-cpu
   ```
   On Windows, use `convert\Scripts\pip`.
2. **Download the model** (16 MB):
   <https://storage.googleapis.com/mediapipe-models/image_segmenter/selfie_multiclass_256x256/float32/latest/selfie_multiclass_256x256.tflite>
3. **Convert it:**
   ```sh
   convert/bin/python -m tf2onnx.convert --tflite selfie_multiclass_256x256.tflite --output selfie_multiclass.onnx --opset 17
   ```
4. **Add it** in Settings → AI models → Add your own model…:

   | Field | Value |
   |---|---|
   | For | people |
   | Name | MediaPipe people |
   | Input size | 256 |
   | Mean | 0,0,0 |
   | Std | 1,1,1 |
   | Photo layout | Colour per pixel (TensorFlow, MediaPipe) |
   | Output | One score per class (softmax) |
   | Map to use | 0 |
   | Invert | on |
   | Licence | Apache-2.0 |

   Then choose the `.onnx` file. Or from a terminal:
   ```sh
   amb models add selfie_multiclass.onnx --task people --name "MediaPipe people" --size 256 \
     --nhwc --softmax --channel 0 --invert --mean 0,0,0 --std 1,1,1 --licence Apache-2.0
   ```
5. **Use it:** click Use next to it (or `amb models use own-mediapipe-people`). People masks now come from it, in well under a second a photo.

The model gives six maps: 0 background, 1 hair, 2 body skin, 3 face skin, 4 clothes, 5 other (accessories). Map 0 inverted is everyone. Another map number gives just that part (for example 1 for hair), but it then takes the People mask's place. Separate masks per part are planned.

## Any other model

A model can be added when it takes a square colour photo and gives a map of the same size, 1 where the thing is. Its fields:

- **For:** `subject` (Subject and Background masks) or `people`.
- **Input size:** the square side, in pixels.
- **Mean and Std:** how it expects colours: each channel, from 0 to 1, has the mean taken off and is divided by the std. ImageNet's values (0.485,0.456,0.406 and 0.229,0.224,0.225) are the usual ones; 0,0,0 and 1,1,1 leave colours at 0 to 1.
- **Photo layout:** colour planes first (most PyTorch and ONNX models, shape 1×3×S×S) or colour per pixel (TensorFlow, shape 1×S×S×3).
- **Output:** a map used as it is (stretched to 0–1), scores that need a sigmoid, or one score per class (softmax).
- **Map to use:** for a model with one map per class, which one, from 0.
- **Invert:** use the opposite, for example of a background class.

Ambrotype runs the model once on a grey picture before adding it, so a wrong setting is reported straight away. Only add models whose licence lets you use them.
