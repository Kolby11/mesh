//! `mesh.shell`: typed shell operations.
//!
//! Each call validates its arguments and the caller's capability once, at the
//! call site, and queues a typed [`FrontendEffect`]. The shell drains the
//! queue with [`ScriptContext::drain_shell_effects`] and lowers the effects
//! directly, so a shell call never becomes JSON, never matches a channel
//! string and is never re-authorized on the way to the host.

use super::super::ScriptError;
use super::super::lookup::lua_err;
use super::*;
use crate::policy::ResourceBudget;
use mesh_core_capability::Capability;
use mesh_core_frontend_abi::{FrontendEffect, SurfaceEffect, SurfaceRole};
use mlua::{Error as LuaError, Table, Value as LuaValue};
use std::sync::{Arc, Mutex};

pub(crate) const SHELL_SURFACE_CAPABILITY: &str = "shell.surface";

/// One queued effect and the bytes it reserved from the realm budget.
#[derive(Debug)]
pub(crate) struct QueuedShellEffect {
    effect: FrontendEffect,
    bytes: usize,
}

#[derive(Clone)]
pub(crate) struct ShellEffectQueue {
    queue: Arc<Mutex<Vec<QueuedShellEffect>>>,
    resources: ResourceBudget,
}

impl std::fmt::Debug for ShellEffectQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShellEffectQueue")
            .field("pending", &self.queue.lock().unwrap().len())
            .finish()
    }
}

fn effect_bytes(effect: &FrontendEffect) -> usize {
    const FIXED: usize = 32;
    FIXED
        + match effect {
            FrontendEffect::Surface(surface) => match surface {
                SurfaceEffect::Toggle { surface_id }
                | SurfaceEffect::Show { surface_id }
                | SurfaceEffect::Hide { surface_id }
                | SurfaceEffect::HidePopover { surface_id, .. }
                | SurfaceEffect::SetRole { surface_id, .. }
                | SurfaceEffect::ToggleRole { surface_id }
                | SurfaceEffect::Position { surface_id, .. } => surface_id.len(),
                SurfaceEffect::SetChildRole {
                    surface_id,
                    node_key,
                    ..
                } => surface_id.len() + node_key.len(),
                SurfaceEffect::ActivatePopover {
                    surface_id,
                    trigger_surface,
                    trigger_key,
                    ..
                } => surface_id.len() + trigger_surface.len() + trigger_key.len(),
            },
            FrontendEffect::SetLocale { locale } => locale.len(),
            FrontendEffect::WriteClipboard { text } => text.len(),
            FrontendEffect::Service(_) | FrontendEffect::Debug(_) => 0,
        }
}

impl ShellEffectQueue {
    pub(crate) fn new(resources: ResourceBudget) -> Self {
        Self {
            queue: Arc::default(),
            resources,
        }
    }

    pub(crate) fn push(&self, effect: FrontendEffect) -> mlua::Result<()> {
        let bytes = effect_bytes(&effect);
        crate::operation::reserve_side_effect(&self.resources, bytes)
            .map_err(LuaError::external)?;
        self.queue
            .lock()
            .unwrap()
            .push(QueuedShellEffect { effect, bytes });
        Ok(())
    }

    pub(crate) fn drain(&self) -> Vec<FrontendEffect> {
        let queued = std::mem::take(&mut *self.queue.lock().unwrap());
        if queued.is_empty() {
            return Vec::new();
        }
        let bytes = queued.iter().map(|queued| queued.bytes).sum();
        crate::operation::release_side_effect(&self.resources, queued.len(), bytes);
        queued.into_iter().map(|queued| queued.effect).collect()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.queue.lock().unwrap().is_empty()
    }
}

fn denied(operation: &str, capability: &str) -> LuaError {
    LuaError::external(ScriptError::CapabilityDenied(format!(
        "mesh.shell.{operation} requires the {capability} capability"
    )))
}

fn string_arg(value: Option<LuaValue>, operation: &str, name: &str) -> mlua::Result<String> {
    match value {
        Some(LuaValue::String(value)) => {
            let value = value.to_str()?.to_string();
            if value.is_empty() {
                Err(LuaError::external(format!(
                    "mesh.shell.{operation}: {name} must not be empty"
                )))
            } else {
                Ok(value)
            }
        }
        _ => Err(LuaError::external(format!(
            "mesh.shell.{operation} expects {name} as a string"
        ))),
    }
}

fn role_arg(value: Option<LuaValue>, operation: &str) -> mlua::Result<SurfaceRole> {
    let role = string_arg(value, operation, "role")?;
    match role.trim().to_ascii_lowercase().as_str() {
        "layer" => Ok(SurfaceRole::Layer),
        "window" | "toplevel" => Ok(SurfaceRole::Window),
        _ => Err(LuaError::external(format!(
            "mesh.shell.{operation}: role must be \"layer\" or \"window\", got {role:?}"
        ))),
    }
}

fn integer_arg(value: Option<LuaValue>, operation: &str, name: &str) -> mlua::Result<i32> {
    let number = match value {
        Some(LuaValue::Integer(value)) => value as f64,
        Some(LuaValue::Number(value)) => value,
        _ => {
            return Err(LuaError::external(format!(
                "mesh.shell.{operation} expects {name} as a number"
            )));
        }
    };
    if !number.is_finite()
        || number.fract() != 0.0
        || number < i32::MIN as f64
        || number > i32::MAX as f64
    {
        return Err(LuaError::external(format!(
            "mesh.shell.{operation}: {name} must be a whole number in the i32 range"
        )));
    }
    Ok(number as i32)
}

fn option_flag(options: Option<&LuaValue>, names: &[&str], default: bool) -> mlua::Result<bool> {
    let Some(LuaValue::Table(table)) = options else {
        return Ok(default);
    };
    for name in names {
        if let Some(value) = table.get::<Option<bool>>(*name)? {
            return Ok(value);
        }
    }
    Ok(default)
}

/// The trigger surface and node key from a pointer or keyboard event table,
/// so a popover can anchor to the element that opened it.
fn trigger_of(event: Option<&LuaValue>) -> (String, String) {
    let Some(LuaValue::Table(event)) = event else {
        return (String::new(), String::new());
    };
    let surface = event
        .get::<Table>("surface")
        .ok()
        .and_then(|surface| surface.get::<String>("id").ok())
        .unwrap_or_default();
    let key = ["current", "current_target"]
        .iter()
        .find_map(|field| {
            event
                .get::<Table>(*field)
                .ok()
                .and_then(|target| target.get::<String>("key").ok())
        })
        .unwrap_or_default();
    (surface, key)
}

pub(crate) fn activate_popover_effect(
    surface_id: String,
    event: Option<&LuaValue>,
    options: Option<&LuaValue>,
) -> mlua::Result<FrontendEffect> {
    let focus = match options {
        Some(LuaValue::Boolean(focus)) => *focus,
        options => option_flag(options, &["focus", "focus_on_open"], true)?,
    };
    let (trigger_surface, trigger_key) = trigger_of(event);
    Ok(FrontendEffect::Surface(SurfaceEffect::ActivatePopover {
        surface_id,
        trigger_surface,
        trigger_key,
        focus,
    }))
}

pub(crate) fn hide_popover_effect(
    surface_id: String,
    options: Option<&LuaValue>,
) -> mlua::Result<FrontendEffect> {
    Ok(FrontendEffect::Surface(SurfaceEffect::HidePopover {
        surface_id,
        defer_for_hover_bridge: option_flag(
            options,
            &["defer", "bridge", "defer_for_hover_bridge"],
            false,
        )?,
    }))
}

type ShellOperationFn = fn(&[LuaValue]) -> mlua::Result<FrontendEffect>;

fn surface_operations() -> [(&'static str, ShellOperationFn); 11] {
    fn arg(args: &[LuaValue], index: usize) -> Option<LuaValue> {
        args.get(index).cloned()
    }
    [
        ("show_surface", |args| {
            let surface_id = string_arg(arg(args, 0), "show_surface", "surface_id")?;
            Ok(FrontendEffect::Surface(SurfaceEffect::Show { surface_id }))
        }),
        ("hide_surface", |args| {
            let surface_id = string_arg(arg(args, 0), "hide_surface", "surface_id")?;
            Ok(FrontendEffect::Surface(SurfaceEffect::Hide { surface_id }))
        }),
        ("toggle_surface", |args| {
            let surface_id = string_arg(arg(args, 0), "toggle_surface", "surface_id")?;
            Ok(FrontendEffect::Surface(SurfaceEffect::Toggle {
                surface_id,
            }))
        }),
        ("hide_popover", |args| {
            let surface_id = string_arg(arg(args, 0), "hide_popover", "surface_id")?;
            hide_popover_effect(surface_id, args.get(1))
        }),
        ("activate_popover", |args| {
            let surface_id = string_arg(arg(args, 0), "activate_popover", "surface_id")?;
            activate_popover_effect(surface_id, args.get(1), args.get(2))
        }),
        ("set_surface_role", |args| {
            let surface_id = string_arg(arg(args, 0), "set_surface_role", "surface_id")?;
            let role = role_arg(arg(args, 1), "set_surface_role")?;
            Ok(FrontendEffect::Surface(SurfaceEffect::SetRole {
                surface_id,
                role,
            }))
        }),
        ("toggle_surface_role", |args| {
            let surface_id = string_arg(arg(args, 0), "toggle_surface_role", "surface_id")?;
            Ok(FrontendEffect::Surface(SurfaceEffect::ToggleRole {
                surface_id,
            }))
        }),
        ("set_widget_role", |args| {
            let surface_id = string_arg(arg(args, 0), "set_widget_role", "surface_id")?;
            let node_key = string_arg(arg(args, 1), "set_widget_role", "node_key")?;
            let role = role_arg(arg(args, 2), "set_widget_role")?;
            Ok(FrontendEffect::Surface(SurfaceEffect::SetChildRole {
                surface_id,
                node_key,
                role,
            }))
        }),
        ("promote_widget", |args| {
            let surface_id = string_arg(arg(args, 0), "promote_widget", "surface_id")?;
            let node_key = string_arg(arg(args, 1), "promote_widget", "node_key")?;
            Ok(FrontendEffect::Surface(SurfaceEffect::SetChildRole {
                surface_id,
                node_key,
                role: SurfaceRole::Window,
            }))
        }),
        ("demote_widget", |args| {
            let surface_id = string_arg(arg(args, 0), "demote_widget", "surface_id")?;
            let node_key = string_arg(arg(args, 1), "demote_widget", "node_key")?;
            Ok(FrontendEffect::Surface(SurfaceEffect::SetChildRole {
                surface_id,
                node_key,
                role: SurfaceRole::Layer,
            }))
        }),
        ("position_surface", |args| {
            let surface_id = string_arg(arg(args, 0), "position_surface", "surface_id")?;
            let margin_top = integer_arg(arg(args, 1), "position_surface", "margin_top")?;
            let margin_left = integer_arg(arg(args, 2), "position_surface", "margin_left")?;
            Ok(FrontendEffect::Surface(SurfaceEffect::Position {
                surface_id,
                margin_top,
                margin_left,
            }))
        }),
    ]
}

impl ScriptContext {
    /// Install `mesh.shell`. The capability is checked here, once; each
    /// function then only checks a captured flag, and raises a capability
    /// error at the call site when the grant is missing.
    pub(super) fn install_shell_api(&mut self, mesh: &Table) -> Result<(), ScriptError> {
        let shell = self.lua().create_table().map_err(lua_err)?;
        let granted = self
            .capabilities
            .is_granted(&Capability::new(SHELL_SURFACE_CAPABILITY));
        for (name, build) in surface_operations() {
            let queue = self.shell_effects.clone();
            shell
                .set(
                    name,
                    self.lua()
                        .create_function(move |_lua, args: mlua::Variadic<LuaValue>| {
                            if !granted {
                                return Err(denied(name, SHELL_SURFACE_CAPABILITY));
                            }
                            queue.push(build(&args)?)
                        })
                        .map_err(lua_err)?,
                )
                .map_err(lua_err)?;
        }
        mesh.set("shell", shell).map_err(lua_err)
    }

    /// Take the typed shell effects queued since the last drain, in call order.
    pub fn drain_shell_effects(&mut self) -> Vec<FrontendEffect> {
        self.shell_effects.drain()
    }

    /// Whether any typed shell effects are queued.
    pub fn has_shell_effects(&self) -> bool {
        !self.shell_effects.is_empty()
    }
}
