mod annotate;
mod fingerprint;
mod node_id;
mod service_deps;
mod tree;

#[cfg(test)]
mod tests;

pub(in crate::shell::component) use annotate::*;
use fingerprint::*;
pub(in crate::shell::component) use node_id::*;
pub(in crate::shell::component) use service_deps::*;
pub(in crate::shell::component) use tree::*;

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use bitflags::bitflags;
use mesh_core_elements::style::{
    BackgroundPaint, Color, ComputedStyle, Corners, Dimension, Edges, Transform2D,
};
use mesh_core_elements::{
    AccessibilityRole, ElementState, NodeId, WidgetNode, WindowSurfaceState, element_snapshot_json,
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
