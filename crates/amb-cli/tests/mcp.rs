// SPDX-License-Identifier: GPL-3.0-or-later
//! `amb mcp` end to end: the real binary over stdio, on a synthetic DNG.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

struct Server {
    child: std::process::Child,
    out: BufReader<std::process::ChildStdout>,
    next: u64,
}

impl Server {
    fn start() -> Server {
        let mut child = Command::new(env!("CARGO_BIN_EXE_amb"))
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("amb mcp starts");
        let out = BufReader::new(child.stdout.take().unwrap());
        Server {
            child,
            out,
            next: 1,
        }
    }

    fn send(&mut self, msg: Value) {
        let stdin = self.child.stdin.as_mut().unwrap();
        writeln!(stdin, "{msg}").unwrap();
        stdin.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        let mut line = String::new();
        self.out.read_line(&mut line).unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap_or_else(|e| panic!("{e}: {line}"));
        assert_eq!(reply["id"], id, "{reply}");
        reply
    }

    fn tool(&mut self, name: &str, arguments: Value) -> Value {
        self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        )["result"]
            .clone()
    }
}

fn text(result: &Value) -> String {
    result["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn edit_describe_preview_and_export_over_stdio() {
    let dir = std::env::temp_dir().join(format!("amb-mcp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let photo = dir.join("a.dng");
    amb_testkit::dng::write(
        &photo,
        &amb_testkit::patterns::generate("ramp").unwrap(),
        &Default::default(),
    )
    .unwrap();
    let p = photo.display().to_string();

    let mut s = Server::start();
    let init = s.request(
        "initialize",
        json!({ "protocolVersion": "2025-06-18", "capabilities": {} }),
    );
    assert_eq!(init["result"]["serverInfo"]["name"], "ambrotype");
    s.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));

    // A dry run writes nothing; the real edit writes the sidecar.
    let cmds = json!([{ "cmd": "set_param", "name": "Exposure2012", "value": 0.7 }]);
    let dry = s.tool(
        "edit_photo",
        json!({ "photo": p, "commands": cmds, "dry_run": true }),
    );
    assert!(text(&dry).contains("dry run"), "{dry}");
    assert!(!dir.join("a.xmp").exists());
    let done = s.tool("edit_photo", json!({ "photo": p, "commands": cmds }));
    assert!(text(&done).contains("Exposure2012"), "{done}");
    assert!(dir.join("a.xmp").is_file());

    let flagged = s.tool(
        "set_photo_info",
        json!({ "photo": p, "rating": 4, "flag": "pick", "add_keywords": ["test"] }),
    );
    assert!(flagged.get("isError").is_none(), "{flagged}");
    let described = s.tool("describe_photo", json!({ "photo": p }));
    let d: Value = serde_json::from_str(&text(&described)).unwrap();
    assert_eq!(
        (d["rating"].clone(), d["flag"].clone()),
        (json!(4), json!("pick"))
    );
    assert!(d["edit"].to_string().contains("Exposure2012"), "{d}");
    let listed: Value = serde_json::from_str(&text(&s.tool(
        "list_photos",
        json!({ "folder": dir.display().to_string() }),
    )))
    .unwrap();
    assert_eq!(listed["photos"].as_array().unwrap().len(), 1);
    assert_eq!(listed["photos"][0]["edited"], true);

    // A preview comes back as a JPEG image.
    let preview = s.tool("render_preview", json!({ "photo": p, "long_edge": 256 }));
    assert!(preview.get("isError").is_none(), "{preview}");
    let image = &preview["content"][0];
    assert_eq!(image["type"], "image");
    assert!(
        image["data"].as_str().unwrap().starts_with("/9j/"),
        "not a JPEG"
    );

    // Export, then refuse to overwrite.
    let out = dir.join("out.jpg").display().to_string();
    let exported = s.tool(
        "export_photo",
        json!({ "photo": p, "out": out, "long_edge": 300 }),
    );
    assert!(exported.get("isError").is_none(), "{exported}");
    assert!(dir.join("out.jpg").is_file());
    let again = s.tool("export_photo", json!({ "photo": p, "out": out }));
    assert_eq!(again["isError"], true);

    drop(s.child.stdin.take());
    let _ = s.child.wait();
    let _ = std::fs::remove_dir_all(&dir);
}
