// SPDX-License-Identifier: GPL-3.0-or-later
//! Answering queries about the settings and the registry.

use super::*;

impl DevelopState {
    // ---- queries -----------------------------------------------------------

    /// Answer a query as JSON.
    pub fn query(&self, q: &Query) -> Result<serde_json::Value, Error> {
        let catalog = Catalog::builtin();
        Ok(match q {
            Query::GetParam { name } => {
                let info = self.info_for(name)?;
                self.param_json(&info, false)
            }
            Query::Describe { name } => {
                let info = self.info_for(name)?;
                self.param_json(&info, true)
            }
            Query::ListParams { group, only_set } => {
                if let Some(g) = group {
                    known_group(g)?;
                }
                let only_set = only_set.unwrap_or(group.is_none());
                let mut names: Vec<String> = match group {
                    Some(g) => catalog.group(g).map(|g| g.tags.clone()).unwrap_or_default(),
                    None => catalog.tags().iter().map(|t| t.name.clone()).collect(),
                };
                // Settings present in the state but not listed in the registry
                // (prefix-rule tags, unknown tags copied from presets).
                let listed: BTreeSet<String> = names.iter().cloned().collect();
                for name in self.settings.keys() {
                    if !listed.contains(name)
                        && group
                            .as_deref()
                            .is_none_or(|g| group_of(name).as_deref() == Some(g))
                    {
                        names.push(name.clone());
                    }
                }
                let params: Vec<serde_json::Value> = names
                    .iter()
                    .filter(|n| !only_set || self.settings.contains_key(*n))
                    .map(|n| match catalog.lookup(n) {
                        Some(info) => self.param_json(&info, false),
                        None => serde_json::json!({
                            "name": n, "group": null, "value": self.settings.get(n),
                        }),
                    })
                    .collect();
                serde_json::json!({ "group": group, "only_set": only_set, "params": params })
            }
            Query::ListMasks => masks::describe(&self.settings),
            Query::ListSpots => spots::describe(&self.settings),
            Query::ListSnapshots => {
                let list: Vec<serde_json::Value> = self
                    .snapshots
                    .iter()
                    .map(|s| {
                        serde_json::json!({
                            "name": s.name,
                            "settings": s.settings.len(),
                            "current": s.settings == self.settings,
                        })
                    })
                    .collect();
                serde_json::json!({ "snapshots": list })
            }
            Query::ListGroups => {
                let groups: Vec<serde_json::Value> = catalog
                    .groups()
                    .iter()
                    .map(|g| {
                        let set = self
                            .settings
                            .keys()
                            .filter(|n| group_of(n).as_deref() == Some(g.name.as_str()))
                            .count();
                        serde_json::json!({
                            "name": g.name, "label": g.label, "description": g.description,
                            "tags": g.tags.len(), "set": set,
                        })
                    })
                    .collect();
                serde_json::json!({ "groups": groups })
            }
        })
    }

    fn info_for(&self, name: &str) -> Result<TagInfo, Error> {
        known(name)
    }

    fn param_json(&self, info: &TagInfo, full: bool) -> serde_json::Value {
        let catalog = Catalog::builtin();
        let value = self.settings.get(&info.name);
        let default = info
            .default
            .clone()
            .filter(|d| !d.is_null())
            .unwrap_or(serde_json::Value::Null);
        let effective = match value {
            Some(v) => serde_json::to_value(v).unwrap_or_default(),
            None => default.clone(),
        };
        let text = catalog.text(&info.name);
        let mut out = serde_json::json!({
            "name": info.name,
            "label": text.map(|t| t.label.clone()),
            "group": info.group,
            "kind": info.kind,
            "min": info.min,
            "max": info.max,
            "default": default,
            "value": value,
            "effective": effective,
        });
        if full {
            let obj = out.as_object_mut().expect("object");
            obj.insert(
                "status".into(),
                serde_json::to_value(info.status).unwrap_or_default(),
            );
            obj.insert(
                "units".into(),
                serde_json::json!(text.and_then(|t| t.units.clone())),
            );
            obj.insert(
                "description".into(),
                serde_json::json!(text.map(|t| t.description.clone())),
            );
            obj.insert(
                "group_label".into(),
                serde_json::json!(catalog.group(&info.group).map(|g| g.label.clone())),
            );
        }
        out
    }
}
