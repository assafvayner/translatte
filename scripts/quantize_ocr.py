# /// script
# requires-python = ">=3.10"
# dependencies = ["onnx>=1.17", "onnxruntime==1.22.1", "numpy", "pillow>=10.1", "scipy"]
# ///
"""Quantize a PP-OCR detector or recognizer ONNX model to int8 for ONNX Runtime.

Static modes calibrate activations on real images: the detector on the images themselves (resized
as `det_preprocess` does), a recognizer on line crops found by the fp32 detector. Weights are
int8 per channel, activations uint8. Quantizing only Conv with percentile calibration keeps CER
within noise of fp32 on the fixtures; the detector's first convs and some of the CJK
recognizer's must stay float (min-max calibration or quantizing them loses lines):

    M=models-cache F=fixtures-cache
    uv run scripts/quantize_ocr.py det $M/det/inference.onnx $M/det-q8/inference.onnx \\
        --ops Conv --method percentile --limit 40 --exclude Conv.0 Conv.1 Conv.2 Conv.3 Conv.4 \\
        --calib $F/*.jpg frames/*.jpg
    uv run scripts/quantize_ocr.py rec $M/rec-latin/inference.onnx $M/rec-latin-q8/inference.onnx \\
        --ops Conv --method percentile --limit 150 --det $M/det/inference.onnx \\
        --calib $F/{fr,de,es,it,pl,pt,id,en}_*.jpg
    uv run scripts/quantize_ocr.py rec $M/rec-cjk/inference.onnx $M/rec-cjk-q8/inference.onnx \\
        --ops Conv --method percentile --percentile 99.95 --limit 250 --det $M/det/inference.onnx \\
        --exclude Conv.{0..5} Conv.{18..23} --calib $F/{zh,ja}_*.jpg frames/*.jpg
"""

import argparse
import random
import sys
import tempfile
from pathlib import Path

import numpy as np
import onnx
from onnx import version_converter
from onnxruntime.quantization import (
    CalibrationDataReader,
    CalibrationMethod,
    QuantFormat,
    QuantType,
    quantize_dynamic,
    quantize_static,
)
from onnxruntime.quantization.shape_inference import quant_pre_process
from PIL import Image
from scipy import ndimage

DET_MEAN = np.array([0.485, 0.456, 0.406], np.float32)
DET_STD = np.array([0.229, 0.224, 0.225], np.float32)
REC_HEIGHT = 48
REC_MIN_WIDTH = 160
REC_MAX_WIDTH = 3200


def det_input(img: Image.Image, long_side: int) -> np.ndarray:
    w, h = img.size
    r = long_side / max(w, h)
    rw, rh = (max(1, round(v * r / 32)) * 32 for v in (w, h))
    x = np.asarray(img.resize((rw, rh), Image.BILINEAR), np.float32) / 255.0
    x = (x - DET_MEAN) / DET_STD
    return x[:, :, ::-1].transpose(2, 0, 1)[None].copy()


def rec_input(crop: Image.Image) -> np.ndarray:
    w, h = crop.size
    rw = min(REC_MAX_WIDTH, max(1, int(np.ceil(REC_HEIGHT * w / h))))
    x = np.asarray(crop.resize((rw, REC_HEIGHT), Image.BILINEAR), np.float32) / 127.5 - 1.0
    out = np.zeros((1, 3, REC_HEIGHT, max(rw, REC_MIN_WIDTH)), np.float32)
    out[0, :, :, :rw] = x[:, :, ::-1].transpose(2, 0, 1)
    return out


def line_crops(sess, img: Image.Image) -> list[Image.Image]:
    """Axis-aligned crops of the detector's text regions, grown as DB's unclip does."""
    x = det_input(img, 960)
    prob = sess.run(None, {sess.get_inputs()[0].name: x})[0][0, 0]
    sx, sy = img.width / prob.shape[1], img.height / prob.shape[0]
    labels, _ = ndimage.label(prob > 0.3)
    crops = []
    for sl in ndimage.find_objects(labels):
        y0, y1, x0, x1 = sl[0].start, sl[0].stop, sl[1].start, sl[1].stop
        bw, bh = x1 - x0, y1 - y0
        if min(bw, bh) < 3:
            continue
        d = bw * bh * 1.5 / (2 * (bw + bh))
        box = (max(0, (x0 - d) * sx), max(0, (y0 - d) * sy), min(img.width, (x1 + d) * sx), min(img.height, (y1 + d) * sy))
        crop = img.crop(tuple(int(round(v)) for v in box))
        if crop.height >= 1.5 * crop.width:
            crop = crop.rotate(90, expand=True)
        crops.append(crop)
    return crops


class Reader(CalibrationDataReader):
    def __init__(self, name: str, inputs: list[np.ndarray]):
        self.it = iter([{name: x} for x in inputs])

    def get_next(self):
        return next(self.it, None)


def calibration_inputs(kind: str, images: list[Path], det: Path | None, limit: int) -> list[np.ndarray]:
    import onnxruntime as ort

    imgs = [Image.open(p).convert("RGB") for p in images]
    if kind == "det":
        inputs = [det_input(img, side) for img in imgs for side in (640, 960)]
    else:
        sess = ort.InferenceSession(str(det), providers=["CPUExecutionProvider"])
        inputs = [rec_input(c) for img in imgs for c in line_crops(sess, img)]
    random.Random(0).shuffle(inputs)
    return inputs[:limit]


def pad_to_common_shape(inputs: list[np.ndarray]) -> list[np.ndarray]:
    """Zero-pads inputs to one shape (on the right and bottom): histogram calibrators stack them."""
    h = max(x.shape[2] for x in inputs)
    w = max(x.shape[3] for x in inputs)
    return [np.pad(x, ((0, 0), (0, 0), (0, h - x.shape[2]), (0, w - x.shape[3]))) for x in inputs]


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("kind", choices=["det", "rec"])
    ap.add_argument("input", type=Path)
    ap.add_argument("output", type=Path)
    ap.add_argument("--mode", choices=["dynamic", "qdq", "qoperator"], default="qdq")
    ap.add_argument("--calib", type=Path, nargs="*", default=[], help="calibration images (static modes)")
    ap.add_argument("--det", type=Path, help="fp32 detector, to find line crops for a recognizer")
    ap.add_argument("--method", choices=["minmax", "entropy", "percentile"], default="minmax")
    ap.add_argument("--percentile", type=float, default=99.999, help="clipping percentile (percentile method)")
    ap.add_argument("--limit", type=int, default=300, help="at most this many calibration inputs")
    ap.add_argument("--ops", nargs="*", help="only quantize these op types (default: all supported)")
    ap.add_argument("--exclude", nargs="*", default=[], help="node names to keep in float")
    ap.add_argument("--per-tensor", action="store_true", help="per-tensor instead of per-channel weights")
    args = ap.parse_args()

    with tempfile.TemporaryDirectory() as tmp:
        # Per-channel QDQ needs opset 13; the Paddle exports are opset 7 (rec) and 11 (det).
        model = onnx.load(str(args.input))
        if model.opset_import[0].version < 13:
            model = version_converter.convert_version(model, 13)
        # The recognizers are IR version 3, where every initializer (which pre-processing makes of
        # their Constant nodes) is also a graph input that calibration would expect to be fed.
        model.ir_version = max(model.ir_version, 7)
        upgraded = Path(tmp) / "opset13.onnx"
        onnx.save(model, str(upgraded))
        prepped = Path(tmp) / "prepped.onnx"
        quant_pre_process(str(upgraded), str(prepped), skip_symbolic_shape=True)
        model = onnx.load(str(prepped))
        inits = {i.name for i in model.graph.initializer}
        graph_inputs = [i for i in model.graph.input if i.name not in inits]
        del model.graph.input[:]
        model.graph.input.extend(graph_inputs)
        onnx.save(model, str(prepped))
        args.output.parent.mkdir(parents=True, exist_ok=True)
        common = dict(per_channel=not args.per_tensor, op_types_to_quantize=args.ops, nodes_to_exclude=args.exclude)
        if args.mode == "dynamic":
            quantize_dynamic(str(prepped), str(args.output), weight_type=QuantType.QInt8, **common)
        else:
            if not args.calib:
                sys.exit("static quantization needs --calib images")
            if args.kind == "rec" and not args.det:
                sys.exit("recognizer calibration needs --det")
            inputs = calibration_inputs(args.kind, args.calib, args.det, args.limit)
            if args.method != "minmax":
                inputs = pad_to_common_shape(inputs)
            print(f"calibrating on {len(inputs)} inputs")
            quantize_static(
                str(prepped),
                str(args.output),
                Reader(graph_inputs[0].name, inputs),
                quant_format=QuantFormat.QDQ if args.mode == "qdq" else QuantFormat.QOperator,
                activation_type=QuantType.QUInt8,
                weight_type=QuantType.QInt8,
                calibrate_method={
                    "minmax": CalibrationMethod.MinMax,
                    "entropy": CalibrationMethod.Entropy,
                    "percentile": CalibrationMethod.Percentile,
                }[args.method],
                extra_options={"CalibPercentile": args.percentile},
                **common,
            )
    print(f"wrote {args.output} ({args.output.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
