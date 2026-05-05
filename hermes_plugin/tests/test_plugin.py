"""Unit tests for the hermes prompt-compress plugin."""

from __future__ import annotations

import json
import unittest
from unittest.mock import MagicMock, patch

from hermes_plugin import (
    COMPRESS_PRESETS,
    COMPRESS_PROMPT_SCHEMA,
    _extract_system_prompts,
    _handle_slash_command,
    _handle_tool,
    _parse_args,
    _pre_llm_call,
    _serialize_conversation,
    check_requirements,
)


# ---------------------------------------------------------------------------
# _extract_system_prompts
# ---------------------------------------------------------------------------

class TestExtractSystemPrompts(unittest.TestCase):
    def test_empty_history(self) -> None:
        self.assertEqual(_extract_system_prompts([]), "")

    def test_no_system_messages(self) -> None:
        history = [
            {"role": "user", "content": "Hello"},
            {"role": "assistant", "content": "Hi there"},
        ]
        self.assertEqual(_extract_system_prompts(history), "")

    def test_single_system_message(self) -> None:
        history = [
            {"role": "system", "content": "You are a helpful assistant."},
            {"role": "user", "content": "Hello"},
        ]
        self.assertEqual(
            _extract_system_prompts(history), "You are a helpful assistant."
        )

    def test_multiple_system_messages(self) -> None:
        history = [
            {"role": "system", "content": "You are a helpful assistant."},
            {"role": "system", "content": "Be concise."},
            {"role": "user", "content": "Hello"},
        ]
        self.assertEqual(
            _extract_system_prompts(history),
            "You are a helpful assistant.\n\nBe concise.",
        )

    def test_multimodal_system_content(self) -> None:
        history = [
            {
                "role": "system",
                "content": [
                    {"type": "text", "text": "You are helpful."},
                    {"type": "image", "url": "http://example.com/img.png"},
                    {"type": "input_text", "text": "Be concise."},
                ],
            },
        ]
        self.assertEqual(
            _extract_system_prompts(history),
            "You are helpful. Be concise.",
        )

    def test_empty_system_content_ignored(self) -> None:
        history = [
            {"role": "system", "content": "   "},
            {"role": "user", "content": "Hello"},
        ]
        self.assertEqual(_extract_system_prompts(history), "")


# ---------------------------------------------------------------------------
# _serialize_conversation
# ---------------------------------------------------------------------------

class TestSerializeConversation(unittest.TestCase):
    def test_basic_messages(self) -> None:
        messages = [
            {"role": "user", "content": "Hello"},
            {"role": "assistant", "content": "Hi"},
        ]
        self.assertEqual(
            _serialize_conversation(messages),
            "USER: Hello\n\nASSISTANT: Hi",
        )

    def test_multimodal_content(self) -> None:
        messages = [
            {
                "role": "user",
                "content": [
                    {"type": "text", "text": "Look at this:"},
                    {"type": "image", "url": "http://example.com/img.png"},
                ],
            },
        ]
        self.assertEqual(_serialize_conversation(messages), "USER: Look at this:")


# ---------------------------------------------------------------------------
# _pre_llm_call
# ---------------------------------------------------------------------------

class TestPreLLMCall(unittest.TestCase):
    def setUp(self) -> None:
        self._patch_compress = patch("hermes_plugin._compress_via_cli")
        self.mock_compress = self._patch_compress.start()

    def tearDown(self) -> None:
        self._patch_compress.stop()

    def test_empty_history_returns_none(self) -> None:
        result = _pre_llm_call(
            session_id="test-1",
            user_message="Hello",
            conversation_history=[],
            is_first_turn=True,
        )
        self.assertIsNone(result)

    def test_first_turn_with_short_system_no_compression(self) -> None:
        """Short system prompts (< 150 chars) are not compressed."""
        history = [
            {"role": "system", "content": "You are helpful."},
            {"role": "user", "content": "Hello"},
        ]
        result = _pre_llm_call(
            session_id="test-2",
            user_message="Hello",
            conversation_history=history,
            is_first_turn=True,
        )
        self.assertIsNone(result)
        self.mock_compress.assert_not_called()

    def test_system_prompt_compression(self) -> None:
        """Long system prompts are compressed on first turn."""
        system_prompt = "You are a very helpful assistant. " * 50
        history = [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": "Hello"},
        ]

        self.mock_compress.return_value = {
            "output": "Compressed system prompt",
            "original_input_tokens": 100,
            "output_tokens": 80,
            "compression_ratio": 0.8,
        }

        result = _pre_llm_call(
            session_id="test-3",
            user_message="Hello",
            conversation_history=history,
            is_first_turn=True,
        )

        self.assertIsInstance(result, dict)
        self.assertIn("system_prompt", result)
        self.assertEqual(result["system_prompt"], "Compressed system prompt")

    def test_system_prompt_not_compressed_if_savings_too_low(self) -> None:
        """If compression saves fewer than 5 tokens, skip it."""
        system_prompt = "You are a very helpful assistant. " * 50
        history = [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": "Hello"},
        ]

        self.mock_compress.return_value = {
            "output": "Same output",
            "original_input_tokens": 100,
            "output_tokens": 96,  # savings = 4
            "compression_ratio": 0.96,
        }

        result = _pre_llm_call(
            session_id="test-4",
            user_message="Hello",
            conversation_history=history,
            is_first_turn=True,
        )

        self.assertIsNone(result)

    def test_context_compression(self) -> None:
        """Old conversation turns are compressed into context."""
        history = [
            {"role": "system", "content": "You are helpful."},
            {"role": "user", "content": "Q1"},
            {"role": "assistant", "content": "A1"},
            {"role": "user", "content": "Q2"},
            {"role": "assistant", "content": "A2"},
            {"role": "user", "content": "Q3"},
            {"role": "assistant", "content": "A3"},
            {"role": "user", "content": "Q4"},
        ]

        self.mock_compress.return_value = {
            "output": "Compressed context",
            "original_input_tokens": 200,
            "output_tokens": 100,
            "compression_ratio": 0.5,
        }

        result = _pre_llm_call(
            session_id="test-5",
            user_message="Q4",
            conversation_history=history,
            is_first_turn=False,
        )

        self.assertIsInstance(result, dict)
        self.assertIn("context", result)
        self.assertIn("Compressed context", result["context"])
        self.mock_compress.assert_called()

    def test_both_system_and_context_compression(self) -> None:
        """Both system prompt and context are compressed when applicable."""
        system_prompt = "You are a very helpful assistant. " * 50
        history = [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": "Q1"},
            {"role": "assistant", "content": "A1"},
            {"role": "user", "content": "Q2"},
            {"role": "assistant", "content": "A2"},
            {"role": "user", "content": "Q3"},
            {"role": "assistant", "content": "A3"},
            {"role": "user", "content": "Q4"},
        ]

        self.mock_compress.return_value = {
            "output": "Compressed output",
            "original_input_tokens": 100,
            "output_tokens": 80,
            "compression_ratio": 0.8,
        }

        result = _pre_llm_call(
            session_id="test-6",
            user_message="Q4",
            conversation_history=history,
            is_first_turn=False,
        )

        self.assertIsInstance(result, dict)
        self.assertIn("system_prompt", result)
        self.assertIn("context", result)
        self.assertEqual(result["system_prompt"], "Compressed output")

    def test_compression_error_is_graceful(self) -> None:
        """Exceptions during compression are caught and return None."""
        system_prompt = "You are a very helpful assistant. " * 50
        history = [
            {"role": "system", "content": system_prompt},
        ]
        self.mock_compress.side_effect = RuntimeError("boom")

        result = _pre_llm_call(
            session_id="test-7",
            user_message="Hello",
            conversation_history=history,
            is_first_turn=True,
        )

        self.assertIsNone(result)


# ---------------------------------------------------------------------------
# Tool handler
# ---------------------------------------------------------------------------

class TestToolHandler(unittest.TestCase):
    def test_schema_has_required_fields(self) -> None:
        self.assertEqual(COMPRESS_PROMPT_SCHEMA["name"], "compress_prompt")
        self.assertIn("parameters", COMPRESS_PROMPT_SCHEMA)
        self.assertIn("text", COMPRESS_PROMPT_SCHEMA["parameters"]["properties"])

    def test_basic_tool_call(self) -> None:
        with patch("hermes_plugin._compress_via_cli") as mock_compress:
            mock_compress.return_value = {
                "output": "Compressed",
                "output_tokens": 5,
                "original_input_tokens": 10,
                "compression_ratio": 0.5,
            }
            result = _handle_tool({"text": "Hello world"})
            data = json.loads(result)
            self.assertEqual(data["output"], "Compressed")
            mock_compress.assert_called_once_with("Hello world", 0.5, "gpt-4", "agent-aware")

    def test_tool_with_preset(self) -> None:
        with patch("hermes_plugin._compress_via_cli") as mock_compress:
            mock_compress.return_value = {
                "output": "Compressed",
                "output_tokens": 5,
                "original_input_tokens": 10,
                "compression_ratio": 0.5,
            }
            result = _handle_tool({"text": "Hello world", "preset": "system"})
            data = json.loads(result)
            self.assertEqual(data["output"], "Compressed")
            mock_compress.assert_called_once_with("Hello world", 0.3, "gpt-4", "agent-aware")

    def test_tool_missing_text(self) -> None:
        result = _handle_tool({})
        data = json.loads(result)
        self.assertIn("error", data)


# ---------------------------------------------------------------------------
# Slash command
# ---------------------------------------------------------------------------

class TestSlashCommand(unittest.TestCase):
    def test_parse_args_basic(self) -> None:
        text, agg, model, mode = _parse_args("hello world")
        self.assertEqual(text, "hello world")
        self.assertEqual(agg, 0.5)
        self.assertEqual(model, "gpt-4")
        self.assertEqual(mode, "agent-aware")

    def test_parse_args_with_flags(self) -> None:
        text, agg, model, mode = _parse_args("hello --aggressiveness 0.7 --model claude-3")
        self.assertEqual(text, "hello")
        self.assertEqual(agg, 0.7)
        self.assertEqual(model, "claude-3")
        self.assertEqual(mode, "agent-aware")

    def test_parse_args_scorer_mode(self) -> None:
        text, agg, model, mode = _parse_args("hello --scorer-mode standard")
        self.assertEqual(text, "hello")
        self.assertEqual(mode, "standard")

    def test_parse_args_empty_raises(self) -> None:
        with self.assertRaises(ValueError):
            _parse_args("")

    def test_slash_command_success(self) -> None:
        with patch("hermes_plugin._compress_via_cli") as mock_compress:
            mock_compress.return_value = {
                "output": "Compressed",
                "output_tokens": 5,
                "original_input_tokens": 10,
                "compression_ratio": 0.5,
            }
            result = _handle_slash_command("hello world")
            self.assertIn("Compressed", result)
            self.assertIn("5", result)
            self.assertIn("10", result)

    def test_slash_command_error(self) -> None:
        with patch("hermes_plugin._compress_via_cli") as mock_compress:
            mock_compress.side_effect = RuntimeError("binary not found")
            result = _handle_slash_command("hello world")
            self.assertIn("Compression error", result)


# ---------------------------------------------------------------------------
# check_requirements
# ---------------------------------------------------------------------------

class TestCheckRequirements(unittest.TestCase):
    def test_returns_bool(self) -> None:
        self.assertIsInstance(check_requirements(), bool)


if __name__ == "__main__":
    unittest.main()
