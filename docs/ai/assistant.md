# The assistant panel

Presets → **Ask AI…** (or Ctrl+Shift+A) opens a small chat beside the photo. You ask in words ("warmer and a bit moodier"), and your own AI model edits the photo with the same settings you use. Code: `crates/amb-gui/src/assistant.rs` and `ui/assistant.js`.

## Which model

- **Anthropic:** your API key and a model name.
- **OpenAI-compatible:** OpenAI with an API key, or a model on your computer through Ollama (the default address, `http://localhost:11434/v1`) or llama.cpp's server. Local models need no key.
- A chat subscription (Claude, ChatGPT) can't be used here. Their desktop apps can drive Ambrotype through `amb mcp` instead ([mcp.md](mcp.md)).
- The key is kept in the system keychain (macOS Keychain, Windows Credential Manager, the Secret Service on Linux). If there is none, the key is used until the app closes and never written to disk.
- Keys are only sent over `https://`, or `http://` to this computer.

## What the model gets

- Once per request: how to answer, then the [core card](core.md) without its command-line lines. About 600 tokens.
- With each request: the photo's name, camera and as-shot white balance, the settings that differ from the defaults, and its numbers as edited now (`amb stats`: L* percentiles, clipping, the midtone cast, chroma).
- The photo itself only while **Show it the photo** is ticked (off at first): a 512 px JPEG of it as edited, with the request and after each checked edit. Only the latest picture is sent, and none stay in the conversation. Pictures cost more tokens and need a model that reads images.
- The conversation so far (up to 16 messages), and it starts again for each photo.

## How it answers

- `amb: ITEMS` edits, in the compact syntax (`amb: exp=+0.3 hl-=20`). Each line is one undo step, named "Assistant" in the History panel. `temp+=500` starts from the as-shot white balance.
- `ref: GROUP` asks for a group's settings; the app answers and asks again.
- `check` asks for the new numbers after an edit; without it the edit ends the turn.
- At most 5 replies per request. The panel shows each edit's net changes, an Undo button for the request's edits, and the tokens used.

## Not yet

- Masks, crop and spots from the assistant: the compact syntax covers single settings, presets and snapshots.
- Streaming answers, and a cost estimate before sending.
