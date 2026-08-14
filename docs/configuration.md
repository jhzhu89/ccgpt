# Configuration and model limits

This guide describes the configuration sources and runtime limits used by ccgpt. It is intended as a stable reference for operating Claude Code through either GitHub Copilot or a direct Responses API backend.

## Configuration precedence

ccgpt reads `~/.ccgptrc`, then applies any `CCGPT_*` process environment variables on top. A non-empty environment value therefore overrides the same value in the file.

| Setting | Default | Purpose |
| --- | --- | --- |
| `CCGPT_API_KEY` | unset | Selects the direct API backend when present; otherwise ccgpt uses Copilot. |
| `CCGPT_BASE_URL` | `https://api.openai.com/v1` | Base URL for the direct Responses API backend. |
| `CCGPT_MODEL_HIGH` | `gpt-5.6-sol` | Direct-backend target for Opus, Fable, Mythos, default, and unknown Claude model names. |
| `CCGPT_MODEL_BALANCED` | `gpt-5.6-terra` | Direct-backend target for Sonnet. |
| `CCGPT_MODEL_FAST` | `gpt-5.6-luna` | Direct-backend target for Haiku. |
| `CCGPT_PORT` | `8000` | Port used when the gateway is started manually. `ccgpt run` uses a free random loopback port. |
| `CCGPT_DEBUG_FILE` | unset | Absolute path for structured JSONL diagnostics. |

Generic OpenAI variables such as `OPENAI_API_KEY` and `OPENAI_BASE_URL` are intentionally ignored.

## Copilot model catalog

The Copilot backend fetches the live `/models` catalog at startup. ccgpt selects the highest numeric Responses-capable Sol, Terra, and Luna models and treats the selected catalog entries as the source of truth for:

- supported reasoning efforts;
- parallel tool-call support;
- maximum context-window tokens;
- maximum prompt tokens;
- maximum output tokens.

These values are runtime data, not permanent constants. For example, the GPT-5.6 entries observed on 2026-08-14 advertised:

| Limit | Tokens |
| --- | ---: |
| Total context window | 1,050,000 |
| Prompt | 922,000 |
| Output | 128,000 |

The prompt and output limits are independent constraints inside the total context window. A nominal one-million-token frontend window therefore does not imply that a one-million-token prompt is accepted upstream.

For direct API backends, ccgpt has no live model catalog. Known model IDs therefore use a built-in limit table as a fallback. The current table covers GPT-5.6 Sol, Terra, and Luna with the values above. Unknown model IDs remain unbounded in ccgpt and are left to the configured provider.

## Claude Code model and compact behavior

A typical large-window Claude Code configuration is:

```json
{
  "model": "opus[1m]",
  "effortLevel": "xhigh"
}
```

`opus[1m]` controls Claude Code's frontend context accounting. It does not override the Copilot model's prompt limit. When using `ccgpt run`, ccgpt reads the selected Copilot tiers' live prompt limits and adds the lowest shared value as Claude Code's `--autocompact` threshold. With the example catalog above, the effective invocation includes:

```text
--autocompact 922000
```

This automatic argument is added when all selected tiers have a known prompt limit, either from the live Copilot catalog or the built-in direct-backend fallback. A user-supplied `--autocompact VALUE` or `--autocompact=VALUE` always wins. Unknown direct-backend models do not receive a guessed threshold.

Claude Code currently accepts explicit thresholds from 100,000 through 1,000,000 tokens. ccgpt does not inject a catalog value below that range and caps a larger value at the CLI maximum.

Requested `max_tokens` values above a model's advertised or known output limit are rejected locally with an Anthropic-compatible `400 invalid_request_error`; ccgpt does not silently clamp the request.

## Token accounting

For completed Responses requests, ccgpt forwards the upstream cumulative `input_tokens` and `output_tokens` values to Claude Code. Streaming responses carry those final totals in the terminal Anthropic `message_delta` usage object.

`POST /v1/messages/count_tokens` is necessarily an estimate because Copilot does not expose a compatible count-tokens endpoint. The estimate uses the normalized Responses `input` and `tools` payload, includes replayed encrypted reasoning and tool history, uses `o200k` text tokenization, and applies a fixed estimate for images. Use the upstream usage reported after a real request as the authoritative value.

## Verification and diagnostics

Run `ccgpt` directly to see the selected backend and Sol/Terra/Luna model IDs. For request-level verification, set:

```bash
CCGPT_DEBUG_FILE=/absolute/path/to/ccgpt-debug.jsonl
```

The JSONL records include model routing, terminal status, the applied automatic compact threshold, and upstream token counts without logging prompts, tool arguments, authorization values, or reasoning payloads. Avoid sharing the file without reviewing it because an upstream error message may echo request content.
