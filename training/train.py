#!/usr/bin/env python3
"""
Fine-tune DistilBERT for token-level binary classification (keep/discard).

Input: JSONL from prepare_dataset.py
Output: Fine-tuned model saved to ./output/
"""

import argparse
import json
from pathlib import Path

import numpy as np
import torch
from datasets import Dataset
from sklearn.metrics import accuracy_score, f1_score, precision_score, recall_score
from transformers import (
    AutoModelForTokenClassification,
    AutoTokenizer,
    DataCollatorForTokenClassification,
    Trainer,
    TrainingArguments,
)


MODEL_NAME = "distilbert-base-uncased"
NUM_LABELS = 2  # 0 = discard, 1 = keep


def load_data(path: str) -> list[dict]:
    records = []
    with open(path) as f:
        for line in f:
            records.append(json.loads(line))
    return records


def tokenize_and_align(examples, tokenizer):
    """Tokenize words and align labels to sub-word tokens."""
    tokenized = tokenizer(
        examples["words"],
        truncation=True,
        padding=False,
        is_split_into_words=True,
        max_length=512,
    )

    all_labels = []
    for i, labels in enumerate(examples["labels"]):
        word_ids = tokenized.word_ids(batch_index=i)
        label_ids = []
        prev_word_id = None
        for word_id in word_ids:
            if word_id is None:
                label_ids.append(-100)  # Special tokens
            elif word_id != prev_word_id:
                label_ids.append(labels[word_id])
            else:
                # Sub-word: same label as first piece
                label_ids.append(labels[word_id])
            prev_word_id = word_id
        all_labels.append(label_ids)

    tokenized["labels"] = all_labels
    return tokenized


def compute_metrics(pred):
    predictions = np.argmax(pred.predictions, axis=-1)
    labels = pred.label_ids

    # Flatten and filter -100
    true_labels = []
    true_preds = []
    for p_seq, l_seq in zip(predictions, labels):
        for p, l in zip(p_seq, l_seq):
            if l != -100:
                true_labels.append(l)
                true_preds.append(p)

    return {
        "accuracy": accuracy_score(true_labels, true_preds),
        "f1": f1_score(true_labels, true_preds, average="binary"),
        "precision": precision_score(true_labels, true_preds, average="binary"),
        "recall": recall_score(true_labels, true_preds, average="binary"),
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--data", required=True, help="JSONL training data")
    parser.add_argument("--output", default="./output", help="Output directory")
    parser.add_argument("--epochs", type=int, default=3)
    parser.add_argument("--batch-size", type=int, default=16)
    parser.add_argument("--lr", type=float, default=5e-5)
    parser.add_argument("--eval-split", type=float, default=0.1)
    args = parser.parse_args()

    print(f"Loading data from {args.data}...")
    records = load_data(args.data)
    print(f"Loaded {len(records)} samples")

    # Create HF dataset
    dataset = Dataset.from_list(records)
    dataset = dataset.train_test_split(test_size=args.eval_split, seed=42)

    print(f"Train: {len(dataset['train'])}, Eval: {len(dataset['test'])}")

    # Load model and tokenizer
    tokenizer = AutoTokenizer.from_pretrained(MODEL_NAME)
    model = AutoModelForTokenClassification.from_pretrained(
        MODEL_NAME,
        num_labels=NUM_LABELS,
    )

    # Tokenize
    tokenized = dataset.map(
        lambda x: tokenize_and_align(x, tokenizer),
        batched=True,
        remove_columns=dataset["train"].column_names,
    )

    data_collator = DataCollatorForTokenClassification(tokenizer)

    training_args = TrainingArguments(
        output_dir=args.output,
        num_train_epochs=args.epochs,
        per_device_train_batch_size=args.batch_size,
        per_device_eval_batch_size=args.batch_size,
        learning_rate=args.lr,
        weight_decay=0.01,
        eval_strategy="epoch",
        save_strategy="epoch",
        load_best_model_at_end=True,
        metric_for_best_model="f1",
        logging_steps=50,
        fp16=torch.cuda.is_available(),
        report_to="none",
    )

    trainer = Trainer(
        model=model,
        args=training_args,
        train_dataset=tokenized["train"],
        eval_dataset=tokenized["test"],
        data_collator=data_collator,
        compute_metrics=compute_metrics,
    )

    print("Starting training...")
    trainer.train()

    print(f"Saving model to {args.output}/best")
    trainer.save_model(f"{args.output}/best")
    tokenizer.save_pretrained(f"{args.output}/best")

    # Final evaluation
    metrics = trainer.evaluate()
    print(f"\nFinal metrics: {metrics}")


if __name__ == "__main__":
    main()
