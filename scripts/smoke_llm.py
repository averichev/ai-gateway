#!/usr/bin/env python3
"""Explicit, potentially paid LLM smoke test through the real Gateway API.

Only technical metadata is printed. No prompts, answers, tool data or tokens.
Python 3.10+, standard library only. No retries, redirects or code execution.
"""

import argparse
import http.client
import json
import math
import os
import re
import socket
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass

MAX_RESPONSE_BYTES = 4 * 1024 * 1024
ERROR_CODES = {
    "invalid_request", "unauthorized", "forbidden", "route_not_found",
    "provider_timeout", "provider_error", "provider_invalid_response",
    "provider_transport_error", "tool_calling_not_supported",
    "gateway_misconfigured", "storage_error",
}
TEXT_REQUEST = [{"role": "user", "content": "Reply with a short greeting."}]
TOOL_MESSAGES = [{
    "role": "user",
    "content": "Use demo_add_numbers with a=12500 and b=7500. After the tool result, reply with only the sum.",
}]
TOOL = {
    "name": "demo_add_numbers",
    "description": "Add exactly two integers for the smoke test.",
    "parameters": {
        "type": "object",
        "properties": {"a": {"type": "integer"}, "b": {"type": "integer"}},
        "required": ["a", "b"],
        "additionalProperties": False,
    },
}


class CheckFailed(Exception):
    """The argument is always a local, static category, never provider text."""


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


@dataclass
class Reply:
    status: int | None
    body: dict
    latency_ms: int
    error: str | None = None


def request_id(body):
    value = body.get("id", body.get("request_id"))
    return value if isinstance(value, str) and re.fullmatch(r"req_[A-Za-z0-9_-]{1,128}", value) else None


def safe_usage(body):
    usage = body.get("usage")
    if not isinstance(usage, dict):
        return None
    return {
        key: value for key, value in usage.items()
        if key in {"input_tokens", "output_tokens"} and type(value) is int and 0 <= value <= 2**31 - 1
    }


def post_generate(url, token, payload, timeout):
    started = time.monotonic()
    status, body, category = None, {}, None
    try:
        request = urllib.request.Request(
            url.rstrip("/") + "/api/v1/generate",
            data=json.dumps(payload).encode("utf-8"),
            headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"},
            method="POST",
        )
        opener = urllib.request.build_opener(NoRedirect())
        try:
            response = opener.open(request, timeout=timeout)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            status = response.code
            raw = response.read(MAX_RESPONSE_BYTES + 1)
        if len(raw) > MAX_RESPONSE_BYTES:
            category = "response_too_large"
        else:
            try:
                decoded = json.loads(raw)
                if not isinstance(decoded, dict):
                    raise ValueError()
                body = decoded
            except (ValueError, UnicodeError, RecursionError):
                category = "invalid_gateway_json"
        if status != 200:
            error = body.get("error")
            code = error.get("code") if isinstance(error, dict) else None
            category = code if isinstance(code, str) and code in ERROR_CODES else "gateway_http_error"
    except (TimeoutError, socket.timeout):
        category = "client_timeout"
    except urllib.error.URLError as error:
        category = "client_timeout" if isinstance(error.reason, TimeoutError) else "gateway_transport_error"
    except (OSError, ValueError, UnicodeError, http.client.HTTPException):
        category = "gateway_transport_error"
    return Reply(status, body, round((time.monotonic() - started) * 1000), category)


def validate_common(reply, alias):
    if reply.error:
        raise CheckFailed(reply.error)
    if reply.status != 200:
        raise CheckFailed("gateway_http_error")
    if not request_id(reply.body):
        raise CheckFailed("missing_request_id")
    if reply.body.get("model") != alias:
        raise CheckFailed("unexpected_model")
    if "usage" in reply.body:
        usage = reply.body["usage"]
        if not isinstance(usage, dict) or safe_usage(reply.body) != usage:
            raise CheckFailed("invalid_usage")


def validate_text(reply, alias):
    validate_common(reply, alias)
    if reply.body.get("finish_reason") not in {"stop", "length"}:
        raise CheckFailed("unexpected_finish_reason")
    text = reply.body.get("output_text")
    if not isinstance(text, str) or not text.strip():
        raise CheckFailed("empty_output_text")
    if reply.body.get("tool_calls"):
        raise CheckFailed("unexpected_tool_calls")


def validate_tool(reply, alias):
    validate_common(reply, alias)
    if reply.body.get("finish_reason") != "tool_calls":
        raise CheckFailed("tool_call_not_returned")
    calls = reply.body.get("tool_calls")
    if not isinstance(calls, list) or len(calls) != 1 or not isinstance(calls[0], dict):
        raise CheckFailed("invalid_tool_calls")
    call = calls[0]
    if call.get("name") != "demo_add_numbers":
        raise CheckFailed("unexpected_tool_name")
    call_id = call.get("id")
    if not isinstance(call_id, str) or not re.fullmatch(r"[!-~]{1,256}", call_id):
        raise CheckFailed("invalid_tool_call_id")
    arguments = call.get("arguments")
    if (not isinstance(arguments, dict) or set(arguments) != {"a", "b"}
            or any(type(value) is not int for value in arguments.values())
            or arguments != {"a": 12500, "b": 7500}):
        raise CheckFailed("invalid_tool_arguments")
    return call


def emit(scenario, reply, alias, check):
    category = None
    try:
        check(reply, alias)
    except CheckFailed as error:
        category = error.args[0]
    report = {
        "scenario": scenario, "success": category is None, "http_status": reply.status,
        "request_id": request_id(reply.body), "model_alias": alias,
        "latency_ms": reply.latency_ms, "usage": safe_usage(reply.body), "error_category": category,
    }
    print(json.dumps(report, ensure_ascii=True))
    return category is None


def run(url, token, alias, timeout):
    text = post_generate(url, token, {"model": alias, "messages": TEXT_REQUEST}, timeout)
    text_ok = emit("text", text, alias, validate_text)
    first = post_generate(url, token, {
        "model": alias, "messages": TOOL_MESSAGES, "tools": [TOOL],
        "tool_choice": {"name": "demo_add_numbers"},
    }, timeout)
    tool_ok = emit("tool_call", first, alias, validate_tool)
    if not tool_ok:
        # A silent model refusal or explicit tool_calling_not_supported is failure.
        return False
    call = validate_tool(first, alias)
    # The only permitted local operation; no eval, exec, subprocess or dynamic dispatch.
    total = call["arguments"]["a"] + call["arguments"]["b"]
    final = post_generate(url, token, {
        "model": alias,
        "messages": [*TOOL_MESSAGES,
            {"role": "assistant", "tool_calls": [call]},
            {"role": "tool", "tool_call_id": call["id"], "content": json.dumps({"sum": total})}],
        "tool_choice": "none",
    }, timeout)

    def validate_final(reply, model):
        validate_text(reply, model)
        if not re.search(r"(?<!\d)20000(?!\d)", re.sub(r"[\s,_]", "", reply.body["output_text"])):
            raise CheckFailed("incorrect_tool_result_answer")

    final_ok = emit("tool_result", final, alias, validate_final)
    return text_ok and tool_ok and final_ok


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default=os.getenv("GATEWAY_URL", "http://127.0.0.1:8080"))
    parser.add_argument("--token", default=os.getenv("GATEWAY_MACHINE_TOKEN"), help="Prefer GATEWAY_MACHINE_TOKEN to avoid shell history/process listing")
    parser.add_argument("--model", default=os.getenv("GATEWAY_MODEL_ALIAS", "smart-default"))
    parser.add_argument("--timeout", default=os.getenv("GATEWAY_TIMEOUT", "90"))
    args = parser.parse_args()
    try:
        url = urllib.parse.urlsplit(args.url)
        timeout = float(args.timeout)
        valid = (url.scheme in {"http", "https"} and url.hostname and not url.username
                 and not url.password and not url.query and not url.fragment
                 and math.isfinite(timeout) and timeout > 0 and args.model.strip()
                 and args.token and all(33 <= ord(char) < 127 for char in args.token))
    except (ValueError, TypeError):
        valid = False
    if not valid:
        print(json.dumps({"success": False, "error_category": "invalid_smoke_configuration"}))
        return 2
    return 0 if run(args.url, args.token, args.model, timeout) else 1


if __name__ == "__main__":
    sys.exit(main())
