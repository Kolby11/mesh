use super::Manifest;
use std::collections::HashMap;

#[derive(Debug, Clone, thiserror::Error)]
pub enum DependencyGraphError {
    #[error("module dependency cycle detected: {cycle:?}")]
    Cycle { cycle: Vec<String> },
}

pub fn validate_module_dependency_graph<'a, I>(manifests: I) -> Result<(), DependencyGraphError>
where
    I: IntoIterator<Item = &'a Manifest>,
{
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum VisitState {
        Visiting,
        Visited,
    }

    let manifest_map: HashMap<String, &Manifest> = manifests
        .into_iter()
        .map(|manifest| (manifest.package.id.clone(), manifest))
        .collect();
    let mut state = HashMap::<String, VisitState>::new();
    let mut stack = Vec::<String>::new();
    let mut module_ids: Vec<String> = manifest_map.keys().cloned().collect();
    module_ids.sort();

    fn adjacency(manifest: &Manifest, known_modules: &HashMap<String, &Manifest>) -> Vec<String> {
        let mut neighbors: Vec<String> = manifest
            .required_module_dependencies()
            .into_iter()
            .filter(|module_id| known_modules.contains_key(module_id))
            .collect();
        neighbors.extend(
            manifest
                .slot_host_dependencies()
                .into_iter()
                .filter(|module_id| known_modules.contains_key(module_id)),
        );
        neighbors.sort();
        neighbors.dedup();
        neighbors
    }

    fn visit(
        module_id: &str,
        manifest_map: &HashMap<String, &Manifest>,
        state: &mut HashMap<String, VisitState>,
        stack: &mut Vec<String>,
    ) -> Result<(), DependencyGraphError> {
        state.insert(module_id.to_string(), VisitState::Visiting);
        stack.push(module_id.to_string());

        for neighbor in adjacency(manifest_map[module_id], manifest_map) {
            match state.get(&neighbor).copied() {
                Some(VisitState::Visited) => continue,
                Some(VisitState::Visiting) => {
                    let cycle_start = stack
                        .iter()
                        .position(|entry| entry == &neighbor)
                        .unwrap_or_default();
                    let mut cycle = stack[cycle_start..].to_vec();
                    cycle.push(neighbor);
                    return Err(DependencyGraphError::Cycle { cycle });
                }
                None => visit(&neighbor, manifest_map, state, stack)?,
            }
        }

        stack.pop();
        state.insert(module_id.to_string(), VisitState::Visited);
        Ok(())
    }

    for module_id in module_ids {
        if state.contains_key(&module_id) {
            continue;
        }
        visit(&module_id, &manifest_map, &mut state, &mut stack)?;
    }

    Ok(())
}
