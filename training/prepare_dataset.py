#!/usr/bin/env python3
"""
Distill token-importance labels from a teacher LLM (Claude/GPT-4).

For each text sample, we ask the teacher to classify every word as
'keep' (1) or 'discard' (0) for maximum compression while preserving meaning.
Output: JSONL with {"text": "...", "labels": [1, 0, 1, ...]}
"""

import argparse
import json
import os
import random
import shutil
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

try:
    import anthropic
except ImportError:
    anthropic = None


TEACHER_PROMPT = """You are a compression expert. Given a text, classify each word as either KEEP (1) or DISCARD (0).
A word should be KEPT if removing it would change the core meaning of the text.
A word should be DISCARDED if it's a filler, redundant, or can be inferred from context.

Respond with ONLY a JSON array of 0s and 1s, one per word, in order. No explanation.

Text words: {words}
Text: {text}"""


def generate_labels_claude(text: str, client, model: str = "claude-sonnet-4-20250514") -> list[int]:
    """Generate labels using Claude API with retry logic."""
    words = text.split()
    prompt = TEACHER_PROMPT.format(words=words, text=text)
    
    max_retries = 3
    for attempt in range(max_retries):
        try:
            response = client.messages.create(
                model=model,
                max_tokens=4096,
                messages=[{"role": "user", "content": prompt}]
            )
            
            # Parse the response text as JSON
            response_text = response.content[0].text.strip()
            labels = json.loads(response_text)
            
            # Validate that it's a list of integers (0s and 1s)
            if not isinstance(labels, list) or not all(isinstance(x, int) and x in [0, 1] for x in labels):
                raise ValueError("Response is not a valid list of 0s and 1s")
            
            # Validate length matches word count
            if len(labels) != len(words):
                raise ValueError(f"Label count mismatch: got {len(labels)}, expected {len(words)}")
            
            return labels
            
        except (json.JSONDecodeError, ValueError, AttributeError, IndexError) as e:
            print(f"  Warning: Parse error on attempt {attempt + 1}: {e}", file=sys.stderr)
            if attempt < max_retries - 1:
                time.sleep(0.5)
            else:
                raise
        except Exception as e:
            print(f"  Warning: API error on attempt {attempt + 1}: {e}", file=sys.stderr)
            if attempt < max_retries - 1:
                time.sleep(0.5)
            else:
                raise
        
        time.sleep(0.5)  # Rate limiting between API calls
    
    raise RuntimeError("Max retries exceeded")


def generate_labels_claude_cli(text: str, model: str = "sonnet") -> list[int]:
    """Generate labels using the Claude CLI (claude -p). Uses your Max plan — no API key needed."""
    words = text.split()
    prompt = TEACHER_PROMPT.format(words=words, text=text)

    claude_bin = shutil.which("claude")
    if not claude_bin:
        raise RuntimeError("claude CLI not found in PATH. Install: https://docs.anthropic.com/en/docs/claude-code")

    max_retries = 3
    for attempt in range(max_retries):
        try:
            # Unset CLAUDECODE to allow nested CLI invocations
            env = {k: v for k, v in os.environ.items() if k != "CLAUDECODE"}
            result = subprocess.run(
                [claude_bin, "-p", prompt, "--model", model, "--output-format", "text"],
                capture_output=True, text=True, timeout=120, env=env,
            )
            if result.returncode != 0:
                raise RuntimeError(f"claude CLI error: {result.stderr.strip()}")

            response_text = result.stdout.strip()
            # Extract JSON array — response may have markdown fences
            if "```" in response_text:
                response_text = response_text.split("```")[1]
                if response_text.startswith("json"):
                    response_text = response_text[4:]
                response_text = response_text.strip()

            labels = json.loads(response_text)

            if not isinstance(labels, list) or not all(isinstance(x, int) and x in [0, 1] for x in labels):
                raise ValueError("Response is not a valid list of 0s and 1s")

            if len(labels) != len(words):
                raise ValueError(f"Label count mismatch: got {len(labels)}, expected {len(words)}")

            return labels

        except (json.JSONDecodeError, ValueError) as e:
            print(f"  Warning: Parse error on attempt {attempt + 1}: {e}", file=sys.stderr)
            if attempt >= max_retries - 1:
                raise
        except subprocess.TimeoutExpired:
            print(f"  Warning: CLI timeout on attempt {attempt + 1}", file=sys.stderr)
            if attempt >= max_retries - 1:
                raise RuntimeError("claude CLI timed out after 120s")
        except Exception as e:
            print(f"  Warning: CLI error on attempt {attempt + 1}: {e}", file=sys.stderr)
            if attempt >= max_retries - 1:
                raise

        time.sleep(1)

    raise RuntimeError("Max retries exceeded")


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
    parser.add_argument("--method", choices=["heuristic", "claude", "claude-cli"], default="heuristic",
                        help="Labeling method (claude-cli uses the Claude Code CLI — no API key needed)")
    parser.add_argument("--max-samples", type=int, default=10000)
    parser.add_argument("--model", default=None,
                        help="Model for labeling. claude: 'claude-sonnet-4-20250514', claude-cli: 'sonnet'")
    parser.add_argument("--batch-size", type=int, default=50,
                        help="Print progress every N samples")
    parser.add_argument("--workers", type=int, default=1,
                        help="Parallel workers for claude/claude-cli methods (default: 1, recommended: 5-10)")
    args = parser.parse_args()
    
    if args.method == "claude" and anthropic is None:
        print("Error: anthropic package required for --method claude. Run: pip install anthropic", file=sys.stderr)
        print("Tip: Use --method claude-cli instead to use the Claude Code CLI (no API key needed).", file=sys.stderr)
        sys.exit(1)

    # Resolve default model per method
    if args.model is None:
        if args.method == "claude":
            args.model = "claude-sonnet-4-20250514"
        elif args.method == "claude-cli":
            args.model = "sonnet"

    input_path = Path(args.input)
    if not input_path.exists():
        print(f"Error: {input_path} not found", file=sys.stderr)
        sys.exit(1)

    lines = input_path.read_text().strip().split("\n")
    random.shuffle(lines)
    lines = lines[:args.max_samples]

    # Filter empty lines upfront
    texts = [line.strip() for line in lines if line.strip()]

    workers = max(1, args.workers)
    print(f"Processing {len(texts)} samples with {args.method} labeler ({workers} workers)...")

    # Initialize Claude API client if needed
    client = None
    if args.method == "claude":
        client = anthropic.Anthropic()

    def label_one(idx_text):
        """Label a single text sample. Returns (index, text, words, labels) or None on skip."""
        idx, text = idx_text
        words = text.split()

        if args.method == "heuristic":
            labels = generate_labels_heuristic(text)
        elif args.method == "claude":
            try:
                labels = generate_labels_claude(text, client, args.model)
            except Exception as e:
                print(f"  Warning: Claude API failed for sample {idx + 1}: {e}", file=sys.stderr)
                labels = generate_labels_heuristic(text)
        elif args.method == "claude-cli":
            try:
                labels = generate_labels_claude_cli(text, args.model)
            except Exception as e:
                print(f"  Warning: Claude CLI failed for sample {idx + 1}: {e}", file=sys.stderr)
                labels = generate_labels_heuristic(text)

        if len(words) != len(labels):
            print(f"  Warning: word/label mismatch at sample {idx + 1}, skipping", file=sys.stderr)
            return None

        return (idx, text, words, labels)

    completed = 0
    fallbacks = 0
    t_start = time.time()

    with open(args.output, "w") as f:
        if workers == 1:
            # Sequential path (heuristic, or single-worker claude)
            for idx, text in enumerate(texts):
                result = label_one((idx, text))
                if result is not None:
                    _, text, words, labels = result
                    f.write(json.dumps({"text": text, "words": words, "labels": labels}) + "\n")
                    f.flush()
                completed += 1
                if completed % args.batch_size == 0:
                    elapsed = time.time() - t_start
                    rate = completed / elapsed
                    eta = (len(texts) - completed) / rate if rate > 0 else 0
                    print(f"  {completed}/{len(texts)} done ({rate:.1f}/s, ETA {eta/60:.0f}m)")
        else:
            # Parallel path — collect results in order
            results = [None] * len(texts)
            with ThreadPoolExecutor(max_workers=workers) as pool:
                futures = {pool.submit(label_one, (i, t)): i for i, t in enumerate(texts)}
                for future in as_completed(futures):
                    idx = futures[future]
                    try:
                        results[idx] = future.result()
                    except Exception as e:
                        print(f"  Warning: Unexpected error at sample {idx + 1}: {e}", file=sys.stderr)
                    completed += 1
                    if completed % args.batch_size == 0:
                        elapsed = time.time() - t_start
                        rate = completed / elapsed
                        eta = (len(texts) - completed) / rate if rate > 0 else 0
                        print(f"  {completed}/{len(texts)} done ({rate:.1f}/s, ETA {eta/60:.0f}m)")

            # Write in original order
            for result in results:
                if result is not None:
                    _, text, words, labels = result
                    f.write(json.dumps({"text": text, "words": words, "labels": labels}) + "\n")

    elapsed = time.time() - t_start
    print(f"Wrote {args.output} ({completed} samples in {elapsed:.0f}s, {completed/elapsed:.1f}/s)")


if __name__ == "__main__":
    main()
