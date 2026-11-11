// SPDX-License-Identifier: GPL-3.0-or-later
//! The guides for AI agents (and people), built into the program: an
//! installed `amb` or editor has no docs folder beside it. `amb docs`
//! prints them, `amb docs --to DIR` writes them out with their links
//! working, the MCP server offers them as resources, and the editor's
//! Connect an AI app shows them.

/// One guide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Guide {
    /// The short name `amb docs NAME` takes.
    pub name: &'static str,
    /// Where it lives under `docs/`, so links between guides work when
    /// they are written out.
    pub path: &'static str,
    pub about: &'static str,
    pub text: &'static str,
}

macro_rules! guide {
    ($name:literal, $path:literal, $about:literal) => {
        Guide {
            name: $name,
            path: $path,
            about: $about,
            text: include_str!(concat!("../../../docs/", $path)),
        }
    };
}

/// Every guide, the one to read first first.
pub const GUIDES: &[Guide] = &[
    guide!(
        "guide",
        "ai/README.md",
        "Driving Ambrotype from an AI or a script: start here"
    ),
    guide!(
        "core",
        "ai/core.md",
        "The core card: the compact edit syntax and the common settings"
    ),
    guide!(
        "features",
        "FEATURES.md",
        "What the app can do, where each feature is and its amb command"
    ),
    guide!(
        "settings",
        "ai/settings-reference.md",
        "Every setting: range, default, what it does"
    ),
    guide!(
        "mcp",
        "ai/mcp.md",
        "The MCP server (amb mcp): its tools and resources"
    ),
    guide!(
        "assistant",
        "ai/assistant.md",
        "The editor's assistant panel"
    ),
    guide!(
        "layouts",
        "ai/ui-layouts.md",
        "Making layouts and themes (\"looks\") for the editor"
    ),
    guide!(
        "interface",
        "ai/custom-interface.md",
        "Writing a whole new front end for the editor"
    ),
    guide!(
        "plugins",
        "ai/plugins.md",
        "Writing a plugin: sandboxed WebAssembly that returns edits"
    ),
    guide!(
        "film-recipes",
        "ai/film-recipes.md",
        "Turning a camera film recipe into settings"
    ),
    guide!(
        "make-it-yours",
        "make-it-yours.md",
        "Layouts, themes and the hub, for people"
    ),
    guide!(
        "own-models",
        "own-models.md",
        "Using your own AI models for masks"
    ),
];

pub fn find(name: &str) -> Option<&'static Guide> {
    let name = name.trim_end_matches(".md");
    GUIDES.iter().find(|g| {
        g.name.eq_ignore_ascii_case(name) || g.path.eq_ignore_ascii_case(&format!("{name}.md"))
    })
}

/// What an agent started in a terminal is told first: where the guides
/// are and what they're for.
pub fn primer(guides: &std::path::Path, photo: Option<&std::path::Path>) -> String {
    let open = photo
        .and_then(|p| p.file_name())
        .map(|n| format!(" The photo open in the editor is {}.", n.to_string_lossy()))
        .unwrap_or_default();
    format!(
        "You are helping with photos in Ambrotype, a raw photo editor. Its amb command edits, \
         previews, organises and exports them; every edit is undoable and shows in the editor at \
         once. First read the guide in {} (or run: amb docs guide), then amb docs features for \
         what the app can do.{open} Show me what you plan before changing photos.",
        guides.join("ai").join("README.md").display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_guide_is_found_by_name_and_has_text() {
        for g in GUIDES {
            assert_eq!(find(g.name), Some(g));
            assert!(g.text.starts_with("# "), "{} starts with its title", g.path);
        }
        assert_eq!(find("ai/README").map(|g| g.name), Some("guide"));
        assert!(find("nope").is_none());
    }

    #[test]
    fn every_guide_for_ai_agents_is_built_in() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/ai");
        for e in std::fs::read_dir(dir).unwrap() {
            let name = format!("ai/{}", e.unwrap().file_name().to_string_lossy());
            assert!(
                GUIDES.iter().any(|g| g.path == name),
                "{name} is missing from GUIDES"
            );
        }
    }

    #[test]
    fn the_guides_link_to_each_other_by_their_paths() {
        // A link from one guide to another must reach it once written out.
        let readme = find("guide").unwrap().text;
        assert!(readme.contains("](../FEATURES.md)") && readme.contains("](core.md)"));
    }
}
