mod annotate;
mod fingerprint;
mod tree;

#[cfg(test)]
mod tests;

pub(in crate::shell::component) use annotate::*;
use fingerprint::*;
pub(in crate::shell::component) use mesh_core_frontend_runtime::NodeServiceFieldDependencies;
pub(in crate::shell::component) use mesh_core_frontend_runtime::{
    child_runtime_node_id, runtime_node_id_for_key, stable_runtime_node_id,
};
pub(in crate::shell::component) use tree::*;

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use bitflags::bitflags;
use mesh_core_elements::style::{
    BackgroundPaint, Color, ComputedStyle, Corners, Dimension, Edges, GradientGeometry, Transform2D,
};
use mesh_core_elements::{
    AccessibilityRole, NodeId, NodeState, WidgetNode, WindowSurfaceState, element_snapshot_json,
};
use mesh_core_interaction::ScrollOffsetState;

#[cfg(test)]
use mesh_core_interaction::node_is_source;

#[cfg(test)]
use mesh_core_interaction::source_element_tag;

use mesh_core_render::{RenderObjectDirtySummary, RenderObjectFingerprint};

use slotmap::{SecondaryMap, SlotMap, new_key_type};

use smallvec::SmallVec;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
