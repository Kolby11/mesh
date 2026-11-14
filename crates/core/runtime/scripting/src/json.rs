use mlua::{Lua, LuaSerdeExt, Table, Value};

const MAX_JSON_BYTES: usize = 1024 * 1024;

pub(crate) fn create_json_library(lua: &Lua) -> mlua::Result<Table> {
    let json = lua.create_table()?;
    json.set("null", Value::NULL)?;
    json.set(
        "decode",
        lua.create_function(|lua, source: mlua::String| {
            if source.as_bytes().len() > MAX_JSON_BYTES {
                return Err(mlua::Error::runtime("mesh.json.decode input exceeds 1 MiB"));
            }
            let value: serde_json::Value =
                serde_json::from_slice(&source.as_bytes()).map_err(mlua::Error::external)?;
            lua.to_value(&value)
        })?,
    )?;
    json.set_readonly(true);
    Ok(json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_service_mock_runs_and_matches_its_contract() {
        use mlua::LuaSerdeExt;

        let contract = mesh_core_service::parse_compiled_contract(
            "mesh.audio",
            "1.0",
            &serde_json::json!({
                "state": {
                    "percent": { "type": "float", "minimum": 5, "maximum": 100 },
                    "sinks": "Sink[]",
                    "route": "Route",
                    "note": "string?"
                },
                "methods": {
                    "current_route": "() -> Route",
                    "set_volume": "(percent: float) -> Result"
                },
                "types": {
                    "Sink": { "name": "string" },
                    "Route": { "target": "Sink", "label": "string?" }
                }
            }),
        )
        .unwrap();
        let lua = Lua::new();
        let mock: Table = lua
            .load(mesh_core_service::generate_luau_mock(&contract))
            .eval()
            .unwrap();
        let types = &contract.schemas.types;

        let state: Table = mock.get("state").unwrap();
        for field in contract.schemas.state_fields.iter() {
            let value: mlua::Value = state.get(field.name.as_str()).unwrap();
            let value: serde_json::Value = lua.from_value(value).unwrap();
            assert!(
                field.type_expr.matches_compiled(&value, types),
                "{}: {value}",
                field.name
            );
        }
        for method in contract.schemas.methods.iter() {
            let function: mlua::Function = mock.get(method.name.as_str()).unwrap();
            let returned: mlua::Value = function.call((mock.clone(), 50.0)).unwrap();
            let returned: serde_json::Value = lua.from_value(returned).unwrap();
            assert!(
                method.returns.as_ref().unwrap().matches_compiled(&returned, types),
                "{}: {returned}",
                method.name
            );
        }
        let calls: Table = mock.get("calls").unwrap();
        assert_eq!(calls.raw_len(), 2);
    }

    #[test]
    fn json_preserves_nested_values_unicode_and_null_array_entries() {
        let lua = Lua::new();
        lua.globals()
            .set("json", create_json_library(&lua).unwrap())
            .unwrap();
        lua.load(r#"
local data = json.decode(' {"items":[null,{"name":"notes \\"quoted\\" 🪟","nested":{"id":-1337}}]} ')
assert(data.items[1] == json.null)
assert(data.items[2].name == 'notes "quoted" 🪟')
assert(data.items[2].nested.id == -1337)
assert(#data.items == 2)
assert(json.decode('true') == true)
assert(json.decode('null') == json.null)
assert(not pcall(function() json.decode('{} trailing') end))
assert(not pcall(function() json.decode('{invalid}') end))
assert(not pcall(function() json.decode(string.rep(' ', 1024 * 1024 + 1)) end))
assert(not pcall(function() json.decode(string.rep('[', 200) .. string.rep(']', 200)) end))
assert(not pcall(function() json.decode = nil end))
"#).exec().unwrap();
    }
}
