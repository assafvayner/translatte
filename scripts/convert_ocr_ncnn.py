# /// script
# requires-python = ">=3.10"
# dependencies = ["pnnx==20260526", "torch", "onnx"]
# ///
"""Convert the PP-OCRv5 detector and script-pack recognizers from ONNX to NCNN with pnnx.

Reads `<models>/<id>/inference.onnx` (as `translatte fetch` installs it) and writes
`inference.ncnn.param` and `inference.ncnn.bin` next to it. Shapes stay dynamic: the detector
takes any HxW, a recognizer any width at height 48. Weights are stored as fp32, so one pair of
files serves both NCNN fp32 and fp16 inference. Every model's input blob is `in0` and its output
`out0`, which core relies on.

    uv run scripts/convert_ocr_ncnn.py                 # det and every script pack
    uv run scripts/convert_ocr_ncnn.py rec-latin det   # just these
"""

import argparse
import ast
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DET_SHAPES = ("[1,3,736,960]f32", "[1,3,544,736]f32")
REC_SHAPES = ("[1,3,48,320]f32", "[1,3,48,640]f32")


def script_packs() -> list[str]:
    """The SCRIPT_PACKS ids from gen_manifest.py, read without importing its dependencies."""
    tree = ast.parse((ROOT / "scripts/gen_manifest.py").read_text(encoding="utf-8"))
    for node in tree.body:
        if isinstance(node, ast.Assign) and any(getattr(t, "id", None) == "SCRIPT_PACKS" for t in node.targets):
            return list(ast.literal_eval(node.value))
    raise SystemExit("SCRIPT_PACKS not found in scripts/gen_manifest.py")


def blobs(param: Path) -> tuple[set[str], set[str], set[str]]:
    """Layer types, Input blobs, and blobs no layer consumes (the outputs) of an NCNN param file."""
    types, inputs, produced, consumed = set(), set(), set(), set()
    for line in param.read_text().splitlines()[2:]:
        parts = line.split()
        kind, n_in, n_out = parts[0], int(parts[2]), int(parts[3])
        types.add(kind)
        ins, outs = parts[4 : 4 + n_in], parts[4 + n_in : 4 + n_in + n_out]
        consumed.update(ins)
        produced.update(outs)
        if kind == "Input":
            inputs.update(outs)
    return types, inputs, produced - consumed


def convert(pnnx: str, onnx: Path, shapes: tuple[str, str]) -> None:
    dest = onnx.parent
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        src = work / "model.onnx"
        shutil.copyfile(onnx, src)
        param, bin_ = work / "model.ncnn.param", work / "model.ncnn.bin"
        args = [
            pnnx,
            str(src),
            f"inputshape={shapes[0]}",
            f"inputshape2={shapes[1]}",
            "fp16=0",
            f"pnnxparam={work / 'model.pnnx.param'}",
            f"pnnxbin={work / 'model.pnnx.bin'}",
            f"pnnxpy={work / 'model_pnnx.py'}",
            f"pnnxonnx={work / 'model.pnnx.onnx'}",
            f"ncnnparam={param}",
            f"ncnnbin={bin_}",
            f"ncnnpy={work / 'model_ncnn.py'}",
        ]
        log = subprocess.run(args, cwd=work, capture_output=True, text=True)
        if log.returncode != 0 or not param.exists() or not bin_.exists():
            sys.stderr.write(log.stdout + log.stderr)
            raise SystemExit(f"pnnx failed on {onnx}")
        types, inputs, outputs = blobs(param)
        if inputs != {"in0"} or outputs != {"out0"}:
            raise SystemExit(f"{onnx}: expected blobs in0 -> out0, got {inputs} -> {outputs}")
        shutil.copyfile(param, dest / "inference.ncnn.param")
        shutil.copyfile(bin_, dest / "inference.ncnn.bin")
    size = (dest / "inference.ncnn.bin").stat().st_size
    print(f"{dest.name}: {size / 1e6:.1f} MB, layers {' '.join(sorted(types))}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("ids", nargs="*", help="model ids (default: det and every script pack)")
    parser.add_argument("--models", type=Path, default=ROOT / "models-cache")
    args = parser.parse_args()
    from pnnx import EXEC_PATH

    ids = args.ids or ["det", *script_packs()]
    for model_id in ids:
        onnx = args.models / model_id / "inference.onnx"
        if not onnx.exists():
            raise SystemExit(f"{onnx} is missing; run `translatte fetch` first")
        convert(EXEC_PATH, onnx, DET_SHAPES if model_id == "det" else REC_SHAPES)


if __name__ == "__main__":
    main()
