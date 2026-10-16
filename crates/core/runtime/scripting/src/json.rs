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
