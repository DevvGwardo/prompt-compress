#!/usr/bin/env python3
"""
Distill token-importance labels from a teacher LLM (Claude/GPT-4).

For each text sample, we ask the teacher to classify every word as
'keep' (1) or 'discard' (0) for maximum compression while preserving meaning.
Output: JSONL with {"text": "...", "labels": [1, 0, 1, ...]}
"""

import argparse
import json
import random
import sys
from pathlib import Path


TEACHER_PROMPT = """You are a compression expert. Given a text, classify each word as either KEEP (1) or DISCARD (0).
A word should be KEPT if removing it would change the core meaning of the text.
A word should be DISCARDED if it's a filler, redundant, or can be inferred from context.

Respond with ONLY a JSON array of 0s and 1s, one per word, in order. No explanation.

Text words: {words}
Text: {text}"""


def generate_labels_heuristic(text: str) -> list[int]:
    """Fallback heuristic labeler (no API needed). Good enough for bootstrapping."""
    stop_words = {
        "a", "an", "the", "is", "are", "was", "were", "be", "been", "being",
        "have", "has", "had", "do", "does", "did", "will", "would", "could",
        "should", "may", "might", "can", "to", "of", "in", "for", "on", "with",
        "at", "by", "from", "as", "into", "through", "during", "before", "after",
        "between", "out", "off", "over", "under", "again", "then", "once",
        "here", "there", "when", "where", "why", "how", "all", "both", "each",
        "few", "more", "most", "other", "some", "such", "no", "nor", "not",
        "only", "own", "same", "so", "than", "too", "very", "just", "because",
        "but", "and", "or", "if", "while", "about", "up", "that", "this",
        "it", "its", "i", "me", "my", "we", "our", "you", "your", "he", "him",
        "his", "she", "her", "they", "them", "their", "also", "still",
    }

    words = text.split()
    labels = []
    for word in words:
        clean = word.strip(".,!?;:\"'()[]{}").lower()
        if clean in stop_words:
            labels.append(0)
        elif word.strip(".,!?;:\"'()[]{}").isdigit():
            labels.append(1)  # Numbers are important
        elif len(clean) <= 2 and clean.isalpha():
            labels.append(0)
        else:
            labels.append(1)

    return labels


def main():
    parser = argparse.ArgumentParser(description="Generate token importance labels")
    parser.add_argument("--input", "-i", required=True, help="Input text file (one sample per line)")
    parser.add_argument("--output", "-o", required=True, help="Output JSONL file")
    parser.add_argument("--method", choices=["heuristic", "claude"], default="heuristic",
                        help="Labeling method")
    parser.add_argument("--max-samples", type=int, default=10000)
    args = parser.parse_args()

    input_path = Path(args.input)
    if not input_path.exists():
        print(f"Error: {input_path} not found", file=sys.stderr)
        sys.exit(1)

    lines = input_path.read_text().strip().split("\n")
    random.shuffle(lines)
    lines = lines[:args.max_samples]

    print(f"Processing {len(lines)} samples with {args.method} labeler...")

    with open(args.output, "w") as f:
        for i, line in enumerate(lines):
            text = line.strip()
            if not text:
                continue

            if args.method == "heuristic":
                labels = generate_labels_heuristic(text)
            elif args.method == "claude":
                # Placeholder for Claude API labeling
                # In production, call anthropic.messages.create() with TEACHER_PROMPT
                print("Claude labeling not yet implemented — use --method heuristic")
                sys.exit(1)

            words = text.split()
            if len(words) != len(labels):
                print(f"Warning: word/label mismatch at line {i}, skipping")
                continue

            record = {"text": text, "words": words, "labels": labels}
            f.write(json.dumps(record) + "\n")

            if (i + 1) % 1000 == 0:
                print(f"  {i + 1}/{len(lines)} done")

    print(f"Wrote {args.output}")


if __name__ == "__main__":
    main()
