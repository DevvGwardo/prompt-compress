#!/usr/bin/env python3
"""
Evaluate a trained model on a held-out test set.

Computes accuracy, F1, precision, recall, and prints sample predictions.
"""

import argparse
import json

import numpy as np
from transformers import AutoModelForTokenClassification, AutoTokenizer, pipeline


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--model-dir", required=True, help="Path to fine-tuned model")
    parser.add_argument("--data", required=True, help="JSONL test data")
    parser.add_argument("--max-samples", type=int, default=500)
    parser.add_argument("--show-samples", type=int, default=5, help="Number of samples to display")
    args = parser.parse_args()

    print(f"Loading model from {args.model_dir}...")
    tokenizer = AutoTokenizer.from_pretrained(args.model_dir)
    model = AutoModelForTokenClassification.from_pretrained(args.model_dir)

    ner_pipe = pipeline(
        "token-classification",
        model=model,
        tokenizer=tokenizer,
        aggregation_strategy="first",
    )

    records = []
    with open(args.data) as f:
        for line in f:
            records.append(json.loads(line))

    records = records[:args.max_samples]
    print(f"Evaluating on {len(records)} samples...")

    total_correct = 0
    total_tokens = 0
    all_true = []
    all_pred = []

    for i, rec in enumerate(records):
        text = rec["text"]
        true_labels = rec["labels"]
        words = rec["words"]

        # Get predictions
        results = ner_pipe(text)

        # Map predictions back to word-level
        pred_labels = [1] * len(words)  # Default: keep
        for r in results:
            if r["entity_group"] == "LABEL_0":
                # Find which word this corresponds to
                word_text = r["word"].strip()
                for j, w in enumerate(words):
                    if w.lower().startswith(word_text.lower()):
                        pred_labels[j] = 0
                        break

        # Compute accuracy
        min_len = min(len(true_labels), len(pred_labels))
        for t, p in zip(true_labels[:min_len], pred_labels[:min_len]):
            total_correct += int(t == p)
            total_tokens += 1
            all_true.append(t)
            all_pred.append(p)

        if i < args.show_samples:
            kept_words = [w for w, l in zip(words, pred_labels) if l == 1]
            print(f"\n--- Sample {i + 1} ---")
            print(f"Original:   {text}")
            print(f"Compressed: {' '.join(kept_words)}")
            ratio = len(kept_words) / len(words) if words else 1
            print(f"Ratio:      {ratio:.1%}")

    accuracy = total_correct / total_tokens if total_tokens else 0
    from sklearn.metrics import f1_score, precision_score, recall_score

    print(f"\n=== Results ({len(records)} samples) ===")
    print(f"Accuracy:  {accuracy:.4f}")
    print(f"F1:        {f1_score(all_true, all_pred, average='binary'):.4f}")
    print(f"Precision: {precision_score(all_true, all_pred, average='binary'):.4f}")
    print(f"Recall:    {recall_score(all_true, all_pred, average='binary'):.4f}")


if __name__ == "__main__":
    main()
