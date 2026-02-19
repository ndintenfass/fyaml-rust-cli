use serde_yaml::{Mapping, Value};

pub fn canonicalize_yaml(value: &Value) -> Value {
    match value {
        Value::Sequence(items) => Value::Sequence(items.iter().map(canonicalize_yaml).collect()),
        Value::Mapping(map) => {
            let mut items: Vec<(Value, Value)> = map
                .iter()
                .map(|(k, v)| (canonicalize_yaml(k), canonicalize_yaml(v)))
                .collect();
            items.sort_by(|(a, _), (b, _)| sort_key_for_yaml(a).cmp(&sort_key_for_yaml(b)));

            let mut out = Mapping::new();
            for (k, v) in items {
                out.insert(k, v);
            }
            Value::Mapping(out)
        }
        _ => value.clone(),
    }
}

fn sort_key_for_yaml(key: &Value) -> Vec<u8> {
    match key {
        Value::String(s) => s.as_bytes().to_vec(),
        _ => serde_yaml::to_string(key).unwrap_or_else(|_| format!("{key:?}")).into_bytes(),
    }
}

pub fn emit_yaml(value: &Value, include_header: bool, version: &str) -> Result<String, serde_yaml::Error> {
    let mut out = String::new();
    if include_header {
        out.push_str(&format!("# packed by fyaml v{version}\n"));
    }
    out.push_str(&serde_yaml::to_string(value)?);
    Ok(out)
}

pub fn emit_json(value: &Value) -> Result<String, serde_json::Error> {
    let json = serde_json::to_value(value)?;
    let canonical = canonicalize_json(json);
    serde_json::to_string_pretty(&canonical)
}

fn canonicalize_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(canonicalize_json).collect())
        }
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::new();
            let mut keys: Vec<String> = map.keys().cloned().collect();
            keys.sort();
            for key in keys {
                let value = map.get(&key).cloned().unwrap_or(serde_json::Value::Null);
                out.insert(key, canonicalize_json(value));
            }
            serde_json::Value::Object(out)
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalize_yaml_orders_map_keys() {
        let value: Value = serde_yaml::from_str("z: 1\na: 2\n").expect("valid yaml");
        let canonical = canonicalize_yaml(&value);
        let emitted = emit_yaml(&canonical, false, "0.1.0").expect("emit yaml");
        let a_pos = emitted.find("a:").expect("a present");
        let z_pos = emitted.find("z:").expect("z present");
        assert!(a_pos < z_pos);
    }

    #[test]
    fn canonicalize_json_orders_keys() {
        let value: Value = serde_yaml::from_str("z: 1\na: 2\n").expect("valid yaml");
        let json = emit_json(&value).expect("emit json");
        let a_pos = json.find("\"a\"").expect("a present");
        let z_pos = json.find("\"z\"").expect("z present");
        assert!(a_pos < z_pos);
    }
}
