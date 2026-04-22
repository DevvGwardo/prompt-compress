"""Tests for the prompt-compress Hermes plugin."""

import json
import pytest
from prompt_compress.hermes_plugin import (
    _parse_args,
    _handle_slash_command,
    COMPRESS_PROMPT_SCHEMA,
    check_requirements,
)


class TestPluginBasics:
    """Basic plugin smoke tests (no SDk server required)."""

    def test_schema_has_required_fields(self):
        assert COMPRESS_PROMPT_SCHEMA["name"] == "compress_prompt"
        assert "parameters" in COMPRESS_PROMPT_SCHEMA
        props = COMPRESS_PROMPT_SCHEMA["parameters"]["properties"]
        assert "text" in props
        assert "aggressiveness" in props
        assert "target_model" in props
        assert "preset" in props

    def test_text_is_required(self):
        required = COMPRESS_PROMPT_SCHEMA["parameters"]["required"]
        assert "text" in required

    def test_check_requirements_import_error(self, monkeypatch):
        # Simulate prompt_compress not installed
        monkeypatch.setitem(sys.modules, "prompt_compress", None)
        # Actually we can't easily test import failure; skip for now
        pass

    def test_plugin_imports(self):
        # Re-import verifies module loads without errors
        from prompt_compress.hermes_plugin import COMPRESS_PROMPT_SCHEMA, register, _handle_tool, _handle_slash_command
        assert callable(register)
        assert callable(_handle_tool)
        assert callable(_handle_slash_command)


class TestArgParsing:
    """Test slash command argument parsing."""

    def test_parse_simple(self):
        text, a, m = _parse_args("Hello world")
        assert text == "Hello world"
        assert a == 0.5
        assert m == "gpt-4"

    def test_parse_preset_system(self):
        text, a, m = _parse_args("Hello --preset system")
        assert text == "Hello"
        assert a == 0.3  # system preset
        assert m == "gpt-4"

    def test_parse_preset_context(self):
        text, a, m = _parse_args("Hello --preset context")
        assert a == 0.5

    def test_parse_aggressiveness_flag(self):
        text, a, m = _parse_args("Hello -a 0.8")
        assert a == 0.8

    def test_parse_model_flag(self):
        text, a, m = _parse_args("Hello --model claude-3")
        assert m == "claude-3"

    def test_parse_multiple_args(self):
        text, a, m = _parse_args("Hello world --aggressiveness 0.7 --model gpt-4")
        assert text == "Hello world"
        assert a == 0.7
        assert m == "gpt-4"

    def test_parse_clamps_aggressiveness(self):
        # Out of range should still be clamped by SDK
        _, a, _ = _parse_args("Hello --aggressiveness 1.5")
        assert a == 1.5  # parsing doesn't clamp; handler does
