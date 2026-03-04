#!/usr/bin/env python3
"""
Export fine-tuned DistilBERT to ONNX format, then optimize + quantize for fast CPU inference.

Output: models/scorer-v0.1.onnx (optimized, INT8 quantized)
"""

import argparse
from pathlib import Path

from optimum.onnxruntime import ORTModelForTokenClassification, ORTOptimizer, ORTQuantizer
from optimum.onnxruntime.configuration import AutoOptimizationConfig, AutoQuantizationConfig
from transformers import AutoTokenizer


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--model-dir", required=True, help="Path to fine-tuned model (from train.py)")
    parser.add_argument("--output", default="../models/scorer-v0.1.onnx", help="Output ONNX path")
    parser.add_argument("--quantize", action="store_true", help="Apply INT8 quantization")
    args = parser.parse_args()

    model_dir = Path(args.model_dir)
    output_dir = Path(args.output).parent
    output_dir.mkdir(parents=True, exist_ok=True)

    print(f"Loading model from {model_dir}...")
    tokenizer = AutoTokenizer.from_pretrained(model_dir)

    # Export to ONNX via optimum
    print("Exporting to ONNX...")
    ort_model = ORTModelForTokenClassification.from_pretrained(
        model_dir,
        export=True,
    )
    ort_model.save_pretrained(str(output_dir / "onnx_raw"))
    tokenizer.save_pretrained(str(output_dir / "onnx_raw"))

    # Optimize
    print("Optimizing ONNX model...")
    optimizer = ORTOptimizer.from_pretrained(str(output_dir / "onnx_raw"))
    optimization_config = AutoOptimizationConfig.O3()
    optimizer.optimize(
        save_dir=str(output_dir / "onnx_optimized"),
        optimization_config=optimization_config,
    )

    if args.quantize:
        print("Quantizing to INT8...")
        quantizer = ORTQuantizer.from_pretrained(str(output_dir / "onnx_optimized"))
        quantization_config = AutoQuantizationConfig.avx512_vnni(is_static=False)
        quantizer.quantize(
            save_dir=str(output_dir / "onnx_quantized"),
            quantization_config=quantization_config,
        )
        final_dir = output_dir / "onnx_quantized"
    else:
        final_dir = output_dir / "onnx_optimized"

    # Copy final model to output path
    import shutil
    model_file = final_dir / "model.onnx"
    if model_file.exists():
        shutil.copy2(model_file, args.output)
        print(f"Saved: {args.output} ({model_file.stat().st_size / 1024 / 1024:.1f} MB)")
    else:
        # optimum may name it differently
        onnx_files = list(final_dir.glob("*.onnx"))
        if onnx_files:
            shutil.copy2(onnx_files[0], args.output)
            print(f"Saved: {args.output}")
        else:
            print(f"Warning: no .onnx file found in {final_dir}")

    print("Done!")


if __name__ == "__main__":
    main()
