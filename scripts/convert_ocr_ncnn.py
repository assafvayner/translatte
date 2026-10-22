# /// script
# requires-python = ">=3.10"
# dependencies = ["pnnx==20260526", "torch", "onnx", "huggingface_hub>=0.25"]
# ///
"""Convert the PP-OCRv5 detector and script-pack recognizers from ONNX to NCNN with pnnx.

Downloads each model's `inference.onnx` from its PaddlePaddle repo at the revision pinned in
scripts/gen_manifest.py and writes `<out>/<id>/inference.ncnn.param` and `.bin`, the layout of the
repo the manifest's NCNN files come from (NCNN_REPO in gen_manifest.py). Shapes stay dynamic: the detector
takes any HxW, a recognizer any width at height 48. Weights are stored as fp32, so one pair of
files serves both NCNN fp32 and fp16 inference. Every model's input blob is `in0` and its output
`out0`, which core relies on.

    uv run scripts/convert_ocr_ncnn.py                 # det and every script pack, into out/ncnn
    uv run scripts/convert_ocr_ncnn.py rec-latin det --out /tmp/ncnn
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


def upstream() -> dict[str, tuple[str, str]]:
    """Model id -> (ONNX repo, pinned revision) for det and every script pack, read from
    gen_manifest.py without importing its dependencies."""
    tree = ast.parse((ROOT / "scripts/gen_manifest.py").read_text(encoding="utf-8"))
    names = {"DET_REPO", "SCRIPT_PACKS", "REVISIONS"}
    found = {}
    for node in tree.body:
        if isinstance(node, ast.Assign):
            for t in node.targets:
                if getattr(t, "id", None) in names:
                    found[t.id] = node.value
    if set(found) != names:
        raise SystemExit(f"{names - set(found)} not found in scripts/gen_manifest.py")
    det_repo = ast.literal_eval(found["DET_REPO"])
    packs = {pack: repo for pack, (repo, _) in ast.literal_eval(found["SCRIPT_PACKS"]).items()}
    # REVISIONS is keyed by the repo constants, so resolve those names first.
    constants = {"DET_REPO": det_repo}
    revisions = {}
    for key, value in zip(found["REVISIONS"].keys, found["REVISIONS"].values):
        repo = constants.get(getattr(key, "id", None)) or (key.value if isinstance(key, ast.Constant) else None)
        if repo:
            revisions[repo] = ast.literal_eval(value)
    repos = {"det": det_repo, **packs}
    return {model_id: (repo, revisions[repo]) for model_id, repo in repos.items()}


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


def convert(pnnx: str, onnx: Path, dest: Path, shapes: tuple[str, str]) -> None:
    dest.mkdir(parents=True, exist_ok=True)
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
    parser.add_argument("--out", type=Path, default=ROOT / "out/ncnn")
    args = parser.parse_args()
    from huggingface_hub import hf_hub_download
    from pnnx import EXEC_PATH

    models = upstream()
    for model_id in args.ids or list(models):
        if model_id not in models:
            raise SystemExit(f"unknown model {model_id}; expected one of {', '.join(models)}")
        repo, revision = models[model_id]
        onnx = Path(hf_hub_download(repo, "inference.onnx", revision=revision))
        convert(EXEC_PATH, onnx, args.out / model_id, DET_SHAPES if model_id == "det" else REC_SHAPES)


if __name__ == "__main__":
    main()
