use mesh_core_elements::VariableStore;

pub(crate) fn eval_expr(expression: &str, store: &dyn VariableStore) -> String {
    if let Some(name) = expression.strip_prefix('#')
        && store.get_ref(name.trim()).is_none()
    {
        return "0".into();
    }
    if !expression.contains([
        ' ', '(', ')', '#', '.', '+', '-', '*', '/', '<', '>', '=', '~',
    ]) && let Some(value) = store.get_ref(expression)
    {
        return value.to_string();
    }
    let Ok(compiled) = mesh_core_expression::compile_expression(expression) else {
        return String::new();
    };
    let mut variables = store.template_locals();
    for name in store.keys() {
        if !variables.contains_key(&name)
            && let Some(value) = store.get_ref(&name).cloned().or_else(|| store.get(&name))
        {
            variables.insert(name, value);
        }
    }
    let value = mesh_core_expression::evaluate_preview(
        &compiled,
        &variables,
        &serde_json::Map::new(),
        |key| store.translate(key),
    )
    .unwrap_or(serde_json::Value::Null);
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(value) => value,
        other => other.to_string(),
    }
}
