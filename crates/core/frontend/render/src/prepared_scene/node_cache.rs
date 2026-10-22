use crate::display_list::DisplayPaintNode;
use mesh_core_elements::NodeId;
use skia_safe::{Drawable, Picture};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::Arc;

const MAX_NODES: usize = 4096;
const MAX_BYTES: usize = 16 * 1024 * 1024;

/// Work performed while composing a complete scene. Reused nodes still add a
/// picture reference; their drawing commands and resources are not prepared again.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScenePreparationMetrics {
    pub recorded_nodes: usize,
    pub reused_nodes: usize,
    pub live_commands: usize,
    pub recorded_operations: usize,
}

#[derive(PartialEq, Eq)]
pub(crate) struct NodePaintKey {
    pub scale: u32,
    pub clip: (i32, i32, i32, i32),
    pub module: Option<String>,
}

#[derive(Clone)]
pub(crate) struct NodePicture {
    pub picture: Picture,
    pub drawable: RefCell<Drawable>,
    pub images: Vec<skia_safe::Image>,
}

struct Entry {
    node: Arc<DisplayPaintNode>,
    key: NodePaintKey,
    recording: Rc<NodePicture>,
    bytes: usize,
}

#[derive(Default)]
pub(crate) struct SceneNodeCache {
    entries: HashMap<NodeId, Entry>,
    order: VecDeque<NodeId>,
    bytes: usize,
    images: HashMap<u32, (usize, usize)>,
    resources: (u64, u64),
    pub metrics: ScenePreparationMetrics,
    #[cfg(test)]
    pub disabled: bool,
}

impl SceneNodeCache {
    pub fn begin(&mut self, revision: u64) {
        self.metrics = ScenePreparationMetrics::default();
        self.sync_resources(revision);
    }

    fn sync_resources(&mut self, revision: u64) {
        let resources = (revision, super::resource_completion_epoch());
        if self.resources != resources {
            self.entries.clear();
            self.order.clear();
            self.images.clear();
            self.bytes = 0;
            self.resources = resources;
        }
    }

    pub fn get(
        &mut self,
        node: &Arc<DisplayPaintNode>,
        scale: u32,
        clip: (i32, i32, i32, i32),
        module: Option<&str>,
    ) -> Option<Rc<NodePicture>> {
        self.sync_resources(mesh_core_resources::resource_revision());
        #[cfg(test)]
        if self.disabled {
            return None;
        }
        let entry = self.entries.get(&node.id)?;
        if !Arc::ptr_eq(&entry.node, node)
            || entry.key.scale != scale
            || entry.key.clip != clip
            || entry.key.module.as_deref() != module
        {
            return None;
        }
        self.metrics.reused_nodes += 1;
        Some(entry.recording.clone())
    }

    pub fn insert(
        &mut self,
        node: &Arc<DisplayPaintNode>,
        key: NodePaintKey,
        recording: Rc<NodePicture>,
        completion_epoch: u64,
    ) {
        self.metrics.recorded_nodes += 1;
        self.metrics.recorded_operations += recording.picture.approximate_op_count();
        // A placeholder or stand-in prepared before an asynchronous completion
        // must never hide the final asset behind an unchanged node identity.
        if super::resources_pending() || completion_epoch != super::resource_completion_epoch() {
            return;
        }
        #[cfg(test)]
        if self.disabled {
            return;
        }
        let bytes = recording.picture.approximate_bytes_used();
        let image_bytes = |image: &skia_safe::Image| {
            (image.width() as usize)
                .saturating_mul(image.height() as usize)
                .saturating_mul(4)
        };
        if bytes.saturating_add(recording.images.iter().map(image_bytes).sum::<usize>()) > MAX_BYTES
        {
            return;
        }
        if !self.remove(node.id) {
            self.order.push_back(node.id);
        }
        self.bytes += bytes;
        for image in &recording.images {
            let (references, size) = self
                .images
                .entry(image.unique_id())
                .or_insert((0, image_bytes(image)));
            if *references == 0 {
                self.bytes += *size;
            }
            *references += 1;
        }
        self.entries.insert(
            node.id,
            Entry {
                node: Arc::clone(node),
                key,
                recording,
                bytes,
            },
        );
        while self.bytes > MAX_BYTES || self.entries.len() > MAX_NODES {
            let Some(id) = self.order.pop_front() else {
                break;
            };
            self.remove(id);
        }
    }

    fn remove(&mut self, id: NodeId) -> bool {
        let Some(old) = self.entries.remove(&id) else {
            return false;
        };
        self.bytes -= old.bytes;
        for image in &old.recording.images {
            let (references, size) = self
                .images
                .get_mut(&image.unique_id())
                .expect("cached image");
            *references -= 1;
            if *references == 0 {
                self.bytes -= *size;
                self.images.remove(&image.unique_id());
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_node_cache_bounds_shared_resources_and_rejects_inflight_results() {
        let _guard = crate::resource_state_test_lock();
        let mut node = mesh_core_elements::WidgetNode::new("box");
        node.computed_style.background_color = mesh_core_elements::Color::WHITE;
        node.layout.width = 8.;
        node.layout.height = 8.;
        let mut list = crate::RetainedDisplayList::default();
        list.update(&node, 8, 8, true, true);
        let payload = list.paint_commands()[0].node.clone();
        let mut surface = skia_safe::surfaces::raster_n32_premul((1024, 1024)).unwrap();
        surface.canvas().clear(skia_safe::Color::RED);
        let image = surface.image_snapshot();
        let mut recorder = skia_safe::PictureRecorder::new();
        recorder
            .begin_recording(skia_safe::Rect::from_wh(8., 8.), false)
            .draw_image(&image, (0., 0.), None);
        let mut drawable = recorder.finish_recording_as_drawable().unwrap();
        let recording = Rc::new(NodePicture {
            picture: drawable.make_picture_snapshot(),
            drawable: RefCell::new(drawable),
            images: vec![image],
        });
        let mut cache = SceneNodeCache::default();
        cache.begin(mesh_core_resources::resource_revision());
        let epoch = super::super::resource_completion_epoch();
        let key = || NodePaintKey {
            scale: 1f32.to_bits(),
            clip: (0, 0, 8, 8),
            module: None,
        };
        for id in 1..=MAX_NODES + 8 {
            let mut node = (*payload).clone();
            node.id = id as u64;
            cache.insert(&Arc::new(node), key(), recording.clone(), epoch);
        }
        assert_eq!(cache.entries.len(), MAX_NODES);
        assert_eq!(cache.order.len(), MAX_NODES);
        assert_eq!(cache.images.len(), 1);
        assert!(cache.bytes <= MAX_BYTES);
        assert!(!cache.entries.contains_key(&1));
        let mut pending = (*payload).clone();
        pending.id = u64::MAX;
        super::super::resource_completed();
        cache.insert(&Arc::new(pending), key(), recording, epoch);
        assert!(!cache.entries.contains_key(&u64::MAX));
        cache.begin(mesh_core_resources::resource_revision());
        assert!(cache.entries.is_empty());
        assert!(cache.images.is_empty());
        assert!(cache.order.is_empty());
        assert_eq!(cache.bytes, 0);
    }
}
