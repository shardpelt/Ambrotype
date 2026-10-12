// SPDX-License-Identifier: GPL-3.0-or-later
//! The assistant panel: the user's own AI model edits the open photo.
//!
//! The model is reached with the user's API key (Anthropic) or through an
//! OpenAI-compatible server (OpenAI, or a local model such as Ollama or
//! llama.cpp). It gets the core card, the photo's numbers
//! (`amb_engine::stats`) and its current settings, and a small JPEG of the
//! photo only when the user ticks "Show it the photo". It answers with
//! `amb:` lines in the compact edit syntax. Those run as ordinary commands,
//! so every change is in the history and can be undone. Keys live in the
//! OS keychain, never in the settings file.

use serde::{Deserialize, Serialize};
use serde_json::json;

/// Most model calls per request (each `ref:` or `check` asks again).
pub const MAX_STEPS: usize = 5;
/// Most earlier messages sent along with a request.
const MAX_HISTORY: usize = 16;
const MAX_TOKENS: u32 = 1024;
const KEYCHAIN_SERVICE: &str = "Ambrotype assistant";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    #[default]
    Anthropic,
    /// OpenAI's chat completions API, as OpenAI, Ollama and llama.cpp serve it.
    OpenAi,
}

/// Which model to ask. The key is kept apart, in the keychain.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Provider {
    pub kind: Kind,
    /// Empty: the provider's usual address (Ollama's for `OpenAi`).
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
}

impl Provider {
    fn base(&self) -> &str {
        let base = self.base_url.trim().trim_end_matches('/');
        match (base.is_empty(), self.kind) {
            (false, _) => base,
            (true, Kind::Anthropic) => "https://api.anthropic.com",
            (true, Kind::OpenAi) => "http://localhost:11434/v1",
        }
    }

    fn endpoint(&self) -> String {
        match self.kind {
            Kind::Anthropic => format!("{}/v1/messages", self.base()),
            Kind::OpenAi => format!("{}/chat/completions", self.base()),
        }
    }

    /// A model on this computer.
    fn is_local(&self) -> bool {
        let url = self.endpoint();
        ["http://localhost", "http://127.0.0.1", "http://[::1]"]
            .iter()
            .any(|p| url.starts_with(p))
    }

    /// The address is safe to send a key to: https, or this computer.
    fn check_address(&self) -> Result<(), String> {
        let url = self.endpoint();
        if url.starts_with("https://") || self.is_local() {
            Ok(())
        } else {
            Err(format!(
                "{url}: use https://, or http:// only for a model on this computer"
            ))
        }
    }

    /// The keychain entry for this provider's key.
    fn key_name(&self) -> String {
        match self.kind {
            Kind::Anthropic => "anthropic".into(),
            Kind::OpenAi => format!("openai {}", self.base()),
        }
    }
}

/// A message of the conversation, as the model sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// `user` or `assistant`.
    pub role: String,
    pub content: String,
    /// A JPEG of the photo (base64) shown with this message. Only the
    /// latest one is sent, and none are kept in the history.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

fn message(role: &str, content: impl Into<String>) -> Message {
    Message {
        role: role.into(),
        content: content.into(),
        image: None,
    }
}

/// The messages as the provider takes them, with the latest picture only.
fn wire_messages(kind: Kind, messages: &[Message]) -> Vec<serde_json::Value> {
    let last_image = messages.iter().rposition(|m| m.image.is_some());
    messages
        .iter()
        .enumerate()
        .map(|(i, m)| match (&m.image, Some(i) == last_image) {
            (Some(data), true) => {
                let picture = match kind {
                    Kind::Anthropic => json!({
                        "type": "image",
                        "source": {"type": "base64", "media_type": "image/jpeg", "data": data},
                    }),
                    Kind::OpenAi => json!({
                        "type": "image_url",
                        "image_url": {"url": format!("data:image/jpeg;base64,{data}")},
                    }),
                };
                json!({
                    "role": m.role,
                    "content": [picture, {"type": "text", "text": m.content}],
                })
            }
            _ => json!({"role": m.role, "content": m.content}),
        })
        .collect()
}

/// The model's answer and what it cost.
#[derive(Debug, Clone, Default)]
pub struct Reply {
    pub text: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
}

/// Ask the model once.
pub fn call(
    provider: &Provider,
    key: Option<&str>,
    system: &str,
    messages: &[Message],
) -> Result<Reply, String> {
    provider.check_address()?;
    if provider.model.trim().is_empty() {
        return Err("choose a model in the assistant's settings".into());
    }
    let key = key.filter(|k| !k.is_empty());
    let url = provider.endpoint();
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(180))
        // A proxy can't reach a model on this computer.
        .try_proxy_from_env(!provider.is_local())
        .build();
    let mut req = agent.post(&url).set("content-type", "application/json");
    let body = match provider.kind {
        Kind::Anthropic => {
            req = req
                .set("x-api-key", key.ok_or("add your API key in the settings")?)
                .set("anthropic-version", "2023-06-01");
            json!({
                "model": provider.model.trim(),
                "max_tokens": MAX_TOKENS,
                "system": system,
                "messages": wire_messages(provider.kind, messages),
            })
        }
        Kind::OpenAi => {
            if let Some(key) = key {
                req = req.set("authorization", &format!("Bearer {key}"));
            }
            let mut all = vec![json!({"role": "system", "content": system})];
            all.extend(wire_messages(provider.kind, messages));
            json!({
                "model": provider.model.trim(),
                "max_tokens": MAX_TOKENS,
                "messages": all,
            })
        }
    };
    let text = match req.send_string(&body.to_string()) {
        Ok(r) => r.into_string().map_err(|e| format!("{url}: {e}"))?,
        Err(ureq::Error::Status(code, r)) => {
            let body = r.into_string().unwrap_or_default();
            let said = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v["error"]["message"].as_str().map(String::from))
                .unwrap_or(body);
            return Err(format!("{url}: {code} {}", said.trim()));
        }
        Err(e) => return Err(format!("{url}: {e}")),
    };
    parse_reply(provider.kind, &text)
}

fn parse_reply(kind: Kind, body: &str) -> Result<Reply, String> {
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("unexpected answer: {e}"))?;
    let tokens = |k: &str| v["usage"][k].as_u64().unwrap_or(0);
    let reply = match kind {
        Kind::Anthropic => Reply {
            text: v["content"]
                .as_array()
                .map(|blocks| {
                    blocks
                        .iter()
                        .filter_map(|b| b["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("")
                })
                .unwrap_or_default(),
            tokens_in: tokens("input_tokens"),
            tokens_out: tokens("output_tokens"),
        },
        Kind::OpenAi => Reply {
            text: v["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            tokens_in: tokens("prompt_tokens"),
            tokens_out: tokens("completion_tokens"),
        },
    };
    if reply.text.trim().is_empty() {
        return Err(format!("the model gave no answer: {}", body.trim()));
    }
    Ok(reply)
}

/// What the model is told once per request: how to answer, then the core
/// card without its command-line lines.
pub fn system_prompt() -> String {
    let card: Vec<String> = amb_protocol::card::core()
        .lines()
        .filter(|l| {
            !["# ", "Edit:", "Check:", "Match a look:"]
                .iter()
                .any(|p| l.starts_with(p))
        })
        .map(|l| match l.strip_prefix("More: `amb ref GROUP`: ") {
            Some(groups) => format!("Groups for ref: {groups}"),
            None => l.to_string(),
        })
        .collect();
    format!(
        "You edit the user's photo in Ambrotype, a raw editor with Lightroom's settings. \
You don't see the photo; you get its numbers: L* percentiles (0 black, 100 white), \
clipping, the midtone cast (a*: green - / magenta +, b*: blue - / yellow +), chroma, \
and the settings that differ from the defaults.\n\
To edit, write a line `amb: ITEMS`, e.g. `amb: exp=+0.3 hl-=20 sat=-10`. \
The items of one line are one undo step.\n\
To see a group's settings, write `ref: GROUP` and stop; the app answers.\n\
To see the new numbers after an edit, add a line `check`; otherwise the edit ends your turn.\n\
A picture, when there is one, is the photo as edited now, small.\n\
At most {MAX_STEPS} replies per request. Keep answers short: say what you changed and why \
in a sentence or two. Don't change crop, masks or spots unless asked.\n\n{}",
        card.join("\n")
    )
}

/// A request from the model in its reply.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// `amb: ITEMS`: edits in the compact syntax.
    Edit(String),
    /// `ref: GROUP`: a group's settings.
    Ref(String),
    /// `check`: the numbers after the edits.
    Check,
}

fn action_line(line: &str) -> Option<Action> {
    let l = line.trim().trim_start_matches(['-', '*', '>', ' ']);
    let l = l.trim_matches('`').trim();
    let lower = l.to_ascii_lowercase();
    if lower == "check" {
        return Some(Action::Check);
    }
    for (prefix, make) in [
        ("amb:", Action::Edit as fn(String) -> Action),
        ("ref:", Action::Ref),
    ] {
        if lower.starts_with(prefix) {
            let rest = l[prefix.len()..].trim().trim_matches('`').trim();
            return (!rest.is_empty()).then(|| make(rest.to_string()));
        }
    }
    None
}

/// The model's requests, in order.
pub fn actions(text: &str) -> Vec<Action> {
    text.lines().filter_map(action_line).collect()
}

/// The reply as shown to the user: without the request lines and the code
/// fences around them.
pub fn shown_text(text: &str) -> String {
    let kept: Vec<&str> = text.lines().filter(|l| action_line(l).is_none()).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < kept.len() {
        // A fence left with nothing inside it goes too.
        if kept[i].trim_start().starts_with("```")
            && kept.get(i + 1).is_some_and(|n| n.trim() == "```")
        {
            i += 2;
            continue;
        }
        out.push(kept[i]);
        i += 1;
    }
    out.join("\n").trim().to_string()
}

/// A line of the conversation as the panel shows it.
#[derive(Debug, Clone, Serialize)]
pub struct Shown {
    /// `assistant`, or `app` for what the app did.
    pub role: String,
    pub text: String,
}

/// What one request did.
#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    /// The whole conversation to send with the next request.
    pub history: Vec<Message>,
    pub shown: Vec<Shown>,
    /// Edit steps made (each one undo step).
    pub edits: usize,
    pub tokens_in: u64,
    pub tokens_out: u64,
}

/// Add a user message, joining it to a user message already last (the
/// conversation must alternate).
fn push_user(history: &mut Vec<Message>, text: String, image: Option<String>) {
    match history.last_mut() {
        Some(m) if m.role == "user" => {
            m.content.push_str("\n\n");
            m.content.push_str(&text);
            if image.is_some() {
                m.image = image;
            }
        }
        _ => history.push(Message {
            image,
            ..message("user", text)
        }),
    }
}

/// Run one request: ask, answer the model's `ref:` and `check` lines and
/// apply its `amb:` lines, for at most [`MAX_STEPS`] replies. `context` gives
/// the photo's numbers and settings now, and `picture` (when given) a JPEG of
/// it; `apply` runs one `amb:` line and says what changed.
pub fn converse(
    mut ask: impl FnMut(&[Message]) -> Result<Reply, String>,
    history: Vec<Message>,
    request: &str,
    mut context: impl FnMut() -> Result<String, String>,
    mut picture: Option<&mut dyn FnMut() -> Result<String, String>>,
    mut apply: impl FnMut(&str) -> Result<String, String>,
) -> Result<Outcome, String> {
    let skip = history.len().saturating_sub(MAX_HISTORY);
    let mut history: Vec<Message> = history.into_iter().skip(skip).collect();
    // The history starts with the user.
    while history.first().is_some_and(|m| m.role != "user") {
        history.remove(0);
    }
    let look = |picture: &mut Option<&mut dyn FnMut() -> Result<String, String>>| {
        picture.as_mut().map(|f| f()).transpose()
    };
    let image = look(&mut picture)?;
    push_user(
        &mut history,
        format!("Photo now:\n{}\n\nRequest: {}", context()?, request.trim()),
        image,
    );
    let mut out = Outcome {
        history: Vec::new(),
        shown: Vec::new(),
        edits: 0,
        tokens_in: 0,
        tokens_out: 0,
    };
    for step in 0..MAX_STEPS {
        let reply = ask(&history)?;
        out.tokens_in += reply.tokens_in;
        out.tokens_out += reply.tokens_out;
        history.push(message("assistant", reply.text.trim()));
        let text = shown_text(&reply.text);
        if !text.is_empty() {
            out.shown.push(Shown {
                role: "assistant".into(),
                text,
            });
        }
        let mut answer = Vec::new();
        let mut again = false;
        let mut edited = false;
        for action in actions(&reply.text) {
            match action {
                Action::Ref(group) => {
                    again = true;
                    let group = group.trim().to_ascii_lowercase();
                    answer.push(amb_protocol::card::group(&group).unwrap_or_else(|| {
                        format!("No group {group:?}; see the groups listed for ref.")
                    }));
                }
                Action::Check => again = true,
                Action::Edit(items) => match apply(&items) {
                    Ok(changed) => {
                        out.edits += 1;
                        edited = true;
                        out.shown.push(Shown {
                            role: "app".into(),
                            text: changed.clone(),
                        });
                        answer.push(format!("Applied `{items}`:\n{changed}"));
                    }
                    Err(e) => {
                        again = true;
                        out.shown.push(Shown {
                            role: "app".into(),
                            text: format!("Couldn't apply {items}: {e}"),
                        });
                        answer.push(format!("`{items}` failed: {e}"));
                    }
                },
            }
        }
        let mut image = None;
        if again && edited {
            answer.push(format!("Photo now:\n{}", context()?));
            image = look(&mut picture)?;
        }
        if !answer.is_empty() {
            // Kept even when the turn ends, so the model knows next time.
            push_user(
                &mut history,
                format!("[app]\n{}", answer.join("\n\n")),
                image,
            );
        }
        if !again {
            break;
        }
        if step + 1 == MAX_STEPS {
            out.shown.push(Shown {
                role: "app".into(),
                text: format!("Stopped after {MAX_STEPS} replies."),
            });
        }
    }
    // Pictures are big and soon out of date: the history keeps the words.
    for m in &mut history {
        m.image = None;
    }
    out.history = history;
    Ok(out)
}

/// The key saved for `provider`, if any.
pub fn load_key(provider: &Provider) -> Option<String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, &provider.key_name())
        .ok()?
        .get_password()
        .ok()
}

/// Save the key in the keychain (an empty key removes it).
pub fn store_key(provider: &Provider, key: &str) -> Result<(), String> {
    let entry =
        keyring::Entry::new(KEYCHAIN_SERVICE, &provider.key_name()).map_err(|e| e.to_string())?;
    if key.is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    } else {
        entry.set_password(key).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_models_requests() {
        let text = "Lifting the shadows a little.\n```\namb: sh+=20 exp=+0.2\n```\n- `ref: tone_curve`\ncheck\nAMB: con=5";
        assert_eq!(
            actions(text),
            [
                Action::Edit("sh+=20 exp=+0.2".into()),
                Action::Ref("tone_curve".into()),
                Action::Check,
                Action::Edit("con=5".into()),
            ]
        );
        assert_eq!(shown_text(text), "Lifting the shadows a little.");
        assert!(actions("Ambient light: fine. checking now").is_empty());
    }

    #[test]
    fn the_prompt_has_the_card_but_not_the_command_line() {
        let p = system_prompt();
        assert!(p.contains("exp Exposure2012"));
        assert!(p.contains("Groups for ref: metadata"));
        assert!(!p.contains("amb do"));
        assert!(amb_protocol::card::estimate_tokens(&p) < 700);
    }

    #[test]
    fn addresses_and_answers() {
        let mut p = Provider {
            kind: Kind::OpenAi,
            ..Default::default()
        };
        assert_eq!(p.endpoint(), "http://localhost:11434/v1/chat/completions");
        assert!(p.check_address().is_ok());
        p.base_url = "http://example.com/v1/".into();
        assert!(p.check_address().is_err());
        p.base_url = "https://example.com/v1/".into();
        assert_eq!(p.endpoint(), "https://example.com/v1/chat/completions");
        let r = parse_reply(
            Kind::OpenAi,
            r#"{"choices":[{"message":{"content":"hi"}}],"usage":{"prompt_tokens":3,"completion_tokens":1}}"#,
        )
        .unwrap();
        assert_eq!((r.text.as_str(), r.tokens_in, r.tokens_out), ("hi", 3, 1));
        let r = parse_reply(
            Kind::Anthropic,
            r#"{"content":[{"type":"text","text":"a"},{"type":"text","text":"b"}],"usage":{"input_tokens":9,"output_tokens":2}}"#,
        )
        .unwrap();
        assert_eq!((r.text.as_str(), r.tokens_in), ("ab", 9));
        assert!(parse_reply(Kind::Anthropic, r#"{"content":[]}"#).is_err());
    }

    #[test]
    fn a_request_runs_edits_and_answers_lookups() {
        // A scripted model: looks up a group, edits and checks, then finishes.
        let replies = [
            "ref: tone",
            "Brighter.\namb: exp=+0.5\ncheck",
            "Looks right now.",
        ];
        let mut seen = Vec::new();
        let mut n = 0;
        let ask = |msgs: &[Message]| {
            seen.push(msgs.to_vec());
            n += 1;
            Ok(Reply {
                text: replies[n - 1].into(),
                tokens_in: 10,
                tokens_out: 2,
            })
        };
        let mut applied = Vec::new();
        let mut pictures = 0;
        let out = converse(
            ask,
            vec![message("assistant", "stray")],
            "make it brighter",
            || Ok("L* p50 40".into()),
            Some(&mut || {
                pictures += 1;
                Ok(format!("jpeg{pictures}"))
            }),
            |items| {
                applied.push(items.to_string());
                Ok("Exposure2012 0→+0.5".into())
            },
        )
        .unwrap();
        assert_eq!(applied, ["exp=+0.5"]);
        assert_eq!((out.edits, out.tokens_in, out.tokens_out), (1, 30, 6));
        assert_eq!(seen[0].len(), 1, "a leading assistant message is dropped");
        assert!(seen[0][0].content.contains("Request: make it brighter"));
        assert!(seen[1][2].content.contains("Exposure2012 -5..5"));
        assert!(seen[2][4].content.contains("Applied `exp=+0.5`"));
        assert!(seen[2][4].content.contains("Photo now"));
        let shown: Vec<_> = out.shown.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(
            shown,
            ["Brighter.", "Exposure2012 0→+0.5", "Looks right now."]
        );
        assert_eq!(out.history.len(), 6);
        // A picture with the request and after the checked edit, none kept.
        assert_eq!(pictures, 2);
        assert_eq!(seen[0][0].image.as_deref(), Some("jpeg1"));
        assert_eq!(seen[2][4].image.as_deref(), Some("jpeg2"));
        let wire = wire_messages(Kind::Anthropic, &seen[2]);
        assert_eq!(
            wire[0]["content"],
            json!(seen[2][0].content),
            "only the latest picture"
        );
        assert_eq!(wire[4]["content"][0]["source"]["data"], "jpeg2");
        assert!(out.history.iter().all(|m| m.image.is_none()));
    }

    #[test]
    fn an_edit_without_check_ends_the_turn() {
        let mut calls = 0;
        let out = converse(
            |_| {
                calls += 1;
                Ok(Reply {
                    text: "amb: sat=-20".into(),
                    ..Default::default()
                })
            },
            Vec::new(),
            "less colour",
            || Ok(String::new()),
            None,
            |_| Ok("Saturation 0→-20".into()),
        )
        .unwrap();
        assert_eq!((calls, out.edits), (1, 1));
        // The result is kept for the next request.
        assert_eq!(out.history.last().unwrap().role, "user");
    }
}
