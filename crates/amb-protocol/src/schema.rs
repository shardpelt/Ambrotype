//! JSON Schemas (draft 2020-12) for [`Command`](crate::Command) and
//! [`Query`](crate::Query), hand-written to match their serde form.

use serde_json::{Value, json};

use crate::catalog::Catalog;

const DIALECT: &str = "https://json-schema.org/draft/2020-12/schema";

fn group_enum() -> Value {
    let names: Vec<&str> = Catalog::builtin()
        .groups()
        .iter()
        .map(|g| g.name.as_str())
        .collect();
    json!({ "type": "string", "enum": names })
}

fn name_prop() -> Value {
    json!({
        "type": "string",
        "minLength": 1,
        "description": "Camera Raw setting (crs:) name, e.g. \"Exposure2012\". See docs/ai/settings-reference.md or `amb settings reference`."
    })
}

fn variant(
    tag_field: &str,
    tag: &str,
    description: &str,
    props: Value,
    required: &[&str],
) -> Value {
    let mut properties = serde_json::Map::new();
    properties.insert(tag_field.into(), json!({ "const": tag }));
    if let Value::Object(p) = props {
        properties.extend(p);
    }
    let mut req = vec![tag_field];
    req.extend_from_slice(required);
    json!({
        "title": tag,
        "description": description,
        "type": "object",
        "properties": properties,
        "required": req,
        "additionalProperties": false,
    })
}

/// Schema for one command object.
pub fn command_schema() -> Value {
    let groups = group_enum();
    json!({
        "$schema": DIALECT,
        "title": "Ambrotype command",
        "description": "One undoable edit of the develop settings. Every command returns a diff ({name, before, after}); numbers are clamped to the setting's range and the clamping is reported.",
        "oneOf": [
            variant("cmd", "set_param", "Set a setting to an absolute value.", json!({
                "name": name_prop(),
                "value": {
                    "description": "Number for sliders (numeric strings like \"+0.5\" accepted), boolean for switches, string for text/enum settings, [[input, output], ...] on a 0-255 scale for point curves.",
                    "type": ["number", "string", "boolean", "array", "object"]
                }
            }), &["name", "value"]),
            variant("cmd", "adjust_param", "Add a delta to a numeric setting (starting from its current value, or its default when unset).", json!({
                "name": name_prop(),
                "delta": { "type": "number" }
            }), &["name", "delta"]),
            variant("cmd", "reset_param", "Remove a setting so its default applies.", json!({
                "name": name_prop()
            }), &["name"]),
            variant("cmd", "reset_group", "Remove every setting of one registry group.", json!({
                "group": groups
            }), &["group"]),
            variant("cmd", "apply_preset", "Apply a preset file (.xmp, .lrtemplate, or a file with embedded XMP). Preset metadata (name, UUID, ...) is never copied. Values are validated like set_param: numbers are clamped (and the clamping reported), values of the wrong kind are skipped with a note.", json!({
                "path": { "type": "string", "minLength": 1 },
                "amount": { "type": "number", "minimum": 0, "maximum": 200, "default": 100, "description": "Preset strength in percent." },
                "include_groups": { "type": "array", "items": groups, "description": "Only apply settings of these groups." },
                "exclude_groups": { "type": "array", "items": groups, "description": "Never apply settings of these groups." }
            }), &["path"]),
            variant("cmd", "undo", "Revert the most recent command.", json!({}), &[]),
            variant("cmd", "redo", "Re-apply the most recently undone command.", json!({}), &[]),
        ]
    })
}

/// Schema for one query object.
pub fn query_schema() -> Value {
    let groups = group_enum();
    json!({
        "$schema": DIALECT,
        "title": "Ambrotype query",
        "description": "A read-only request about the develop settings.",
        "oneOf": [
            variant("query", "get_param", "Current value, default, effective value and range of one setting.", json!({
                "name": name_prop()
            }), &["name"]),
            variant("query", "list_params", "Settings with current values. Without group: only settings present in the state; with group: every setting of the group. only_set overrides.", json!({
                "group": groups,
                "only_set": { "type": "boolean" }
            }), &[]),
            variant("query", "describe", "Full documentation of one setting plus its current value.", json!({
                "name": name_prop()
            }), &["name"]),
            variant("query", "list_groups", "All setting groups with labels, descriptions and counts.", json!({}), &[]),
        ]
    })
}

/// One document with both schemas under `$defs`; the root accepts a command,
/// an array of commands, or a query.
pub fn protocol_schema() -> Value {
    let strip = |mut v: Value| {
        if let Value::Object(m) = &mut v {
            m.remove("$schema");
        }
        v
    };
    json!({
        "$schema": DIALECT,
        "title": "Ambrotype protocol",
        "description": "Commands and queries over Camera Raw develop settings. Used by `amb edit --commands` (a command or an array of commands) and `amb query --queries`.",
        "$defs": {
            "Command": strip(command_schema()),
            "CommandList": { "type": "array", "items": { "$ref": "#/$defs/Command" } },
            "Query": strip(query_schema()),
            "QueryList": { "type": "array", "items": { "$ref": "#/$defs/Query" } },
        },
        "anyOf": [
            { "$ref": "#/$defs/Command" },
            { "$ref": "#/$defs/CommandList" },
            { "$ref": "#/$defs/Query" },
            { "$ref": "#/$defs/QueryList" },
        ]
    })
}

#[cfg(test)]
mod tests {
    //! A minimal validator for the subset of JSON Schema used above, so the
    //! schemas are checked against real serialised commands without pulling in
    //! a schema crate.
    use super::*;
    use crate::command::{Command, Query};

    fn type_ok(t: &str, v: &Value) -> bool {
        match t {
            "object" => v.is_object(),
            "array" => v.is_array(),
            "string" => v.is_string(),
            "number" => v.is_number(),
            "boolean" => v.is_boolean(),
            "null" => v.is_null(),
            other => panic!("unsupported type {other}"),
        }
    }

    fn validate(schema: &Value, root: &Value, v: &Value) -> bool {
        let s = schema.as_object().expect("schema object");
        if let Some(r) = s.get("$ref") {
            let path = r.as_str().unwrap().trim_start_matches("#/");
            let target = path.split('/').fold(root, |node, key| &node[key]);
            return validate(target, root, v);
        }
        for key in s.keys() {
            assert!(
                [
                    "$schema",
                    "$defs",
                    "title",
                    "description",
                    "type",
                    "const",
                    "enum",
                    "properties",
                    "required",
                    "additionalProperties",
                    "oneOf",
                    "anyOf",
                    "items",
                    "minimum",
                    "maximum",
                    "default",
                    "minLength",
                    "$ref",
                ]
                .contains(&key.as_str()),
                "unsupported keyword {key}"
            );
        }
        if let Some(t) = s.get("type") {
            let ok = match t {
                Value::String(t) => type_ok(t, v),
                Value::Array(ts) => ts.iter().any(|t| type_ok(t.as_str().unwrap(), v)),
                _ => panic!("bad type"),
            };
            if !ok {
                return false;
            }
        }
        if let Some(c) = s.get("const") {
            if c != v {
                return false;
            }
        }
        if let Some(e) = s.get("enum") {
            if !e.as_array().unwrap().contains(v) {
                return false;
            }
        }
        if let (Some(min), Some(n)) = (s.get("minimum"), v.as_f64()) {
            if n < min.as_f64().unwrap() {
                return false;
            }
        }
        if let (Some(max), Some(n)) = (s.get("maximum"), v.as_f64()) {
            if n > max.as_f64().unwrap() {
                return false;
            }
        }
        if let (Some(min), Some(text)) = (s.get("minLength"), v.as_str()) {
            if (text.chars().count() as u64) < min.as_u64().unwrap() {
                return false;
            }
        }
        if let Some(obj) = v.as_object() {
            let props = s.get("properties").and_then(Value::as_object);
            if let Some(req) = s.get("required") {
                for r in req.as_array().unwrap() {
                    if !obj.contains_key(r.as_str().unwrap()) {
                        return false;
                    }
                }
            }
            for (k, val) in obj {
                match props.and_then(|p| p.get(k)) {
                    Some(ps) => {
                        if !validate(ps, root, val) {
                            return false;
                        }
                    }
                    None if s.get("additionalProperties") == Some(&Value::Bool(false)) => {
                        return false;
                    }
                    None => {}
                }
            }
        }
        if let (Some(items), Some(arr)) = (s.get("items"), v.as_array()) {
            if !arr.iter().all(|i| validate(items, root, i)) {
                return false;
            }
        }
        if let Some(one) = s.get("oneOf") {
            let n = one
                .as_array()
                .unwrap()
                .iter()
                .filter(|sub| validate(sub, root, v))
                .count();
            if n != 1 {
                return false;
            }
        }
        if let Some(any) = s.get("anyOf") {
            if !any
                .as_array()
                .unwrap()
                .iter()
                .any(|sub| validate(sub, root, v))
            {
                return false;
            }
        }
        true
    }

    fn all_commands() -> Vec<Command> {
        vec![
            Command::SetParam {
                name: "Exposure2012".into(),
                value: json!(0.5),
            },
            Command::SetParam {
                name: "ToneCurvePV2012".into(),
                value: json!([[0, 20], [255, 255]]),
            },
            Command::AdjustParam {
                name: "Contrast2012".into(),
                delta: -10.0,
            },
            Command::ResetParam {
                name: "Vibrance".into(),
            },
            Command::ResetGroup {
                group: "hsl".into(),
            },
            Command::ApplyPreset {
                path: "look.xmp".into(),
                amount: 50.0,
                include_groups: Some(vec!["tone".into()]),
                exclude_groups: None,
            },
            Command::ApplyPreset {
                path: "look.xmp".into(),
                amount: 100.0,
                include_groups: None,
                exclude_groups: None,
            },
            Command::Undo,
            Command::Redo,
        ]
    }

    #[test]
    fn serialised_commands_and_queries_validate() {
        let cs = command_schema();
        for c in all_commands() {
            let v = serde_json::to_value(&c).unwrap();
            assert!(validate(&cs, &cs, &v), "{v}");
        }
        let qs = query_schema();
        let queries = [
            Query::GetParam {
                name: "Tint".into(),
            },
            Query::ListParams {
                group: Some("tone".into()),
                only_set: Some(true),
            },
            Query::ListParams {
                group: None,
                only_set: None,
            },
            Query::Describe {
                name: "Dehaze".into(),
            },
            Query::ListGroups,
        ];
        for q in queries {
            let v = serde_json::to_value(&q).unwrap();
            assert!(validate(&qs, &qs, &v), "{v}");
        }
    }

    #[test]
    fn schema_rejects_what_serde_rejects() {
        let cs = command_schema();
        let bad = [
            json!({"cmd": "set_param", "name": "Exposure2012"}),
            json!({"cmd": "set_param", "name": "Exposure2012", "value": 1, "extra": 1}),
            json!({"cmd": "adjust_param", "name": "Exposure2012", "delta": "1"}),
            json!({"cmd": "reset_group", "group": "not_a_group"}),
            json!({"cmd": "apply_preset", "path": "p.xmp", "amount": 250}),
            json!({"cmd": "explode"}),
        ];
        for v in bad {
            assert!(!validate(&cs, &cs, &v), "schema accepted {v}");
            // serde rejects these too, except the range/group checks that the
            // state machine enforces.
        }
        assert!(serde_json::from_value::<Command>(json!({"cmd": "explode"})).is_err());
    }

    #[test]
    fn protocol_document_resolves_refs() {
        let doc = protocol_schema();
        let list = json!([{"cmd": "undo"}, {"cmd": "set_param", "name": "Tint", "value": 5}]);
        assert!(validate(&doc, &doc, &list));
        assert!(validate(&doc, &doc, &json!({"query": "list_groups"})));
        assert!(!validate(&doc, &doc, &json!({"query": "nope"})));
        assert_eq!(
            doc["$defs"]["Command"]["oneOf"].as_array().unwrap().len(),
            7
        );
        assert_eq!(doc["$defs"]["Query"]["oneOf"].as_array().unwrap().len(), 4);
    }
}
