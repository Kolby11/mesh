use crate::shell::Shell;
use mesh_core_debug::{
    ProfilingAllocationSample, ProfilingAllocationSummary, ProfilingAttributionSummary,
    ProfilingBackendSample, ProfilingBackendSnapshot, ProfilingBackendStage,
    ProfilingBackendStageSummary, ProfilingInvalidationSnapshot, ProfilingSample,
    ProfilingScopeSnapshot, ProfilingSnapshot, ProfilingStage, ProfilingStageSummary,
    ProfilingSurfaceSnapshot, ProfilingWasteSummary, allocation::AllocationCounters,
};
use mesh_core_render::DebugPerfHudSnapshot;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::time::{Duration, Instant};

const DEFAULT_RECENT_CAPACITY: usize = 16;
const ATTRIBUTION_PREFIX: &str = "attribution:";
const WASTE_PREFIX: &str = "waste:";
const MAX_ATTRIBUTION_ENTRIES: usize = 10;

#[derive(Debug, Default)]
struct StageAccumulator {
    sample_count: u64,
    total_micros: u64,
    max_micros: u64,
    recent_samples: VecDeque<ProfilingSample>,
}

impl StageAccumulator {
    fn record(&mut self, sample: ProfilingSample, recent_capacity: usize) {
        self.sample_count = self.sample_count.saturating_add(1);
        self.total_micros = self.total_micros.saturating_add(sample.duration_micros);
        self.max_micros = self.max_micros.max(sample.duration_micros);
        if recent_capacity > 0 {
            if self.recent_samples.len() >= recent_capacity {
                self.recent_samples.pop_front();
            }
            self.recent_samples.push_back(sample);
        }
    }

    fn snapshot(&self, stage: ProfilingStage) -> ProfilingStageSummary {
        ProfilingStageSummary {
            stage,
            sample_count: self.sample_count,
            total_micros: self.total_micros,
            max_micros: self.max_micros,
            recent_samples: self.recent_samples.iter().cloned().collect(),
        }
    }
}

#[derive(Debug, Default)]
struct ScopeAccumulator {
    stages: BTreeMap<ProfilingStage, StageAccumulator>,
    attribution: BTreeMap<(ProfilingStage, String), StageAccumulator>,
    wasted_work_avoided: BTreeMap<String, u64>,
    redraw_count: u64,
    total_surface_render_time_micros: u64,
    allocations: AllocationAccumulator,
}

impl ScopeAccumulator {
    fn record_stage(&mut self, sample: ProfilingSample, recent_capacity: usize) {
        if let Some(kind) = sample
            .trigger_kind
            .as_deref()
            .and_then(|trigger| trigger.strip_prefix(WASTE_PREFIX))
        {
            let count = self.wasted_work_avoided.entry(kind.to_owned()).or_default();
            *count = count.saturating_add(1);
            return;
        }
        if sample.stage == ProfilingStage::RedrawCount {
            self.redraw_count = self
                .redraw_count
                .saturating_add(u64::from(sample.redraw_count.unwrap_or(1)));
        }
        if sample.stage == ProfilingStage::TotalSurfaceRender {
            self.total_surface_render_time_micros = self
                .total_surface_render_time_micros
                .saturating_add(sample.duration_micros);
        }
        if let Some(key) = sample
            .trigger_kind
            .as_deref()
            .and_then(|trigger| trigger.strip_prefix(ATTRIBUTION_PREFIX))
        {
            self.attribution
                .entry((sample.stage, key.to_owned()))
                .or_default()
                .record(sample.clone(), 0);
        }
        self.stages
            .entry(sample.stage)
            .or_default()
            .record(sample, recent_capacity);
    }

    fn snapshot(&self) -> ProfilingScopeSnapshot {
        let mut attribution = self
            .attribution
            .iter()
            .map(|((stage, key), accumulator)| ProfilingAttributionSummary {
                stage: *stage,
                key: key.clone(),
                sample_count: accumulator.sample_count,
                total_micros: accumulator.total_micros,
                max_micros: accumulator.max_micros,
            })
            .collect::<Vec<_>>();
        attribution.sort_by(|left, right| {
            right
                .total_micros
                .cmp(&left.total_micros)
                .then_with(|| left.stage.cmp(&right.stage))
                .then_with(|| left.key.cmp(&right.key))
        });
        attribution.truncate(MAX_ATTRIBUTION_ENTRIES);
        ProfilingScopeSnapshot {
            stages: self
                .stages
                .iter()
                .map(|(stage, summary)| summary.snapshot(*stage))
                .collect(),
            attribution,
            wasted_work_avoided: self
                .wasted_work_avoided
                .iter()
                .map(|(kind, count)| ProfilingWasteSummary {
                    kind: kind.clone(),
                    count: *count,
                })
                .collect(),
            redraw_count: self.redraw_count,
            total_surface_render_time_micros: self.total_surface_render_time_micros,
            allocations: self.allocations.snapshot(),
        }
    }
}

#[derive(Debug, Default)]
struct AllocationAccumulator {
    sample_count: u64,
    allocation_count: u64,
    allocated_bytes: u64,
    deallocation_count: u64,
    deallocated_bytes: u64,
    reallocation_count: u64,
    max_allocated_bytes_per_pass: u64,
    recent_samples: VecDeque<ProfilingAllocationSample>,
}

impl AllocationAccumulator {
    fn record(&mut self, sample: ProfilingAllocationSample, recent_capacity: usize) {
        self.sample_count = self.sample_count.saturating_add(1);
        self.allocation_count = self
            .allocation_count
            .saturating_add(sample.allocation_count);
        self.allocated_bytes = self.allocated_bytes.saturating_add(sample.allocated_bytes);
        self.deallocation_count = self
            .deallocation_count
            .saturating_add(sample.deallocation_count);
        self.deallocated_bytes = self
            .deallocated_bytes
            .saturating_add(sample.deallocated_bytes);
        self.reallocation_count = self
            .reallocation_count
            .saturating_add(sample.reallocation_count);
        self.max_allocated_bytes_per_pass = self
            .max_allocated_bytes_per_pass
            .max(sample.allocated_bytes);
        if recent_capacity > 0 {
            if self.recent_samples.len() >= recent_capacity {
                self.recent_samples.pop_front();
            }
            self.recent_samples.push_back(sample);
        }
    }

    fn snapshot(&self) -> Option<ProfilingAllocationSummary> {
        (self.sample_count > 0).then(|| ProfilingAllocationSummary {
            sample_count: self.sample_count,
            allocation_count: self.allocation_count,
            allocated_bytes: self.allocated_bytes,
            deallocation_count: self.deallocation_count,
            deallocated_bytes: self.deallocated_bytes,
            reallocation_count: self.reallocation_count,
            max_allocated_bytes_per_pass: self.max_allocated_bytes_per_pass,
            recent_samples: self.recent_samples.iter().cloned().collect(),
        })
    }
}

#[derive(Debug, Default)]
struct SurfaceAccumulator {
    module_id: Option<String>,
    scope: ScopeAccumulator,
    invalidation: Option<ProfilingInvalidationSnapshot>,
}

#[derive(Debug, Default)]
struct BackendStageAccumulator {
    sample_count: u64,
    total_micros: u64,
    max_micros: u64,
    recent_samples: VecDeque<ProfilingBackendSample>,
}

impl BackendStageAccumulator {
    fn record(&mut self, sample: ProfilingBackendSample, recent_capacity: usize) {
        self.sample_count = self.sample_count.saturating_add(1);
        self.total_micros = self.total_micros.saturating_add(sample.duration_micros);
        self.max_micros = self.max_micros.max(sample.duration_micros);
        if self.recent_samples.len() >= recent_capacity {
            self.recent_samples.pop_front();
        }
        self.recent_samples.push_back(sample);
    }

    fn snapshot(&self, stage: ProfilingBackendStage) -> ProfilingBackendStageSummary {
        ProfilingBackendStageSummary {
            stage,
            sample_count: self.sample_count,
            total_micros: self.total_micros,
            max_micros: self.max_micros,
            recent_samples: self.recent_samples.iter().cloned().collect(),
        }
    }
}

#[derive(Debug, Default)]
struct BackendAccumulator {
    stages: BTreeMap<ProfilingBackendStage, BackendStageAccumulator>,
}

#[derive(Debug)]
pub(crate) struct ProfilingRuntimeState {
    session_id: u64,
    session_started: Instant,
    next_sample_order: u64,
    recent_capacity: usize,
    shell: ScopeAccumulator,
    surfaces: HashMap<String, SurfaceAccumulator>,
    backends: BTreeMap<(String, String), BackendAccumulator>,
}

impl Default for ProfilingRuntimeState {
    fn default() -> Self {
        Self {
            session_id: 0,
            session_started: Instant::now(),
            next_sample_order: 0,
            recent_capacity: DEFAULT_RECENT_CAPACITY,
            shell: ScopeAccumulator::default(),
            surfaces: HashMap::new(),
            backends: BTreeMap::new(),
        }
    }
}

impl ProfilingRuntimeState {
    pub(crate) fn perf_hud_snapshot(&self, surface_id: &str) -> DebugPerfHudSnapshot {
        let Some(surface) = self.surfaces.get(surface_id) else {
            return DebugPerfHudSnapshot::default();
        };
        let mut frame_times_micros = [0; 16];
        let frame_time_count = surface
            .scope
            .stages
            .get(&ProfilingStage::TotalSurfaceRender)
            .map_or(0, |stage| {
                let count = stage.recent_samples.len().min(16);
                let skip = stage.recent_samples.len().saturating_sub(count);
                for (destination, sample) in frame_times_micros
                    .iter_mut()
                    .zip(stage.recent_samples.iter().skip(skip))
                {
                    *destination = sample.duration_micros;
                }
                count
            });
        let Some(invalidation) = surface.invalidation.as_ref() else {
            return DebugPerfHudSnapshot {
                frame_times_micros,
                frame_time_count,
                redraw_count: surface.scope.redraw_count,
                ..DebugPerfHudSnapshot::default()
            };
        };
        let retained = &invalidation.retained;
        let paint = &invalidation.paint;
        DebugPerfHudSnapshot {
            frame_times_micros,
            frame_time_count,
            redraw_count: surface.scope.redraw_count,
            retained_generation: invalidation.retained_generation,
            dirty_nodes: retained
                .inserted
                .saturating_add(retained.removed)
                .saturating_add(retained.layout)
                .saturating_add(retained.style)
                .saturating_add(retained.attributes)
                .saturating_add(retained.children)
                .saturating_add(retained.state),
            entries_rebuilt: paint.entries_rebuilt,
            damage_rect_count: paint.damage_rect_count,
            damage_area: paint.damage_area,
            surface_area: paint.surface_area,
            full_surface_damage: paint.full_surface_damage,
        }
    }

    pub(crate) fn reset_for_new_session(&mut self, session_id: u64) {
        self.session_id = session_id;
        self.session_started = Instant::now();
        self.next_sample_order = 0;
        self.shell = ScopeAccumulator::default();
        self.surfaces.clear();
        self.backends.clear();
    }

    pub(crate) fn snapshot(&self, session_id: u64) -> ProfilingSnapshot {
        let mut surfaces: Vec<_> = self
            .surfaces
            .iter()
            .map(|(surface_id, surface)| {
                let scope = surface.scope.snapshot();
                ProfilingSurfaceSnapshot {
                    surface_id: surface_id.clone(),
                    module_id: surface.module_id.clone(),
                    stages: scope.stages,
                    attribution: scope.attribution,
                    wasted_work_avoided: scope.wasted_work_avoided,
                    redraw_count: surface.scope.redraw_count,
                    total_surface_render_time_micros: surface
                        .scope
                        .total_surface_render_time_micros,
                    invalidation: surface.invalidation.clone(),
                    allocations: scope.allocations,
                }
            })
            .collect();
        surfaces.sort_by(|a, b| a.surface_id.cmp(&b.surface_id));

        let backends = self
            .backends
            .iter()
            .map(
                |((interface, provider_id), backend)| ProfilingBackendSnapshot {
                    interface: interface.clone(),
                    provider_id: provider_id.clone(),
                    stages: backend
                        .stages
                        .iter()
                        .map(|(stage, summary)| summary.snapshot(*stage))
                        .collect(),
                },
            )
            .collect();

        ProfilingSnapshot {
            session_id: session_id.max(self.session_id),
            allocation_profiling_available: mesh_core_debug::allocation::profiling_available(),
            shell: self.shell.snapshot(),
            surfaces,
            backends,
        }
    }

    pub(crate) fn record_surface_allocations(
        &mut self,
        surface_id: &str,
        module_id: Option<&str>,
        counters: AllocationCounters,
    ) {
        let module_id = module_id.filter(|id| !id.is_empty());
        let sample = ProfilingAllocationSample {
            order: self.next_sample_order,
            timestamp_micros: self
                .session_started
                .elapsed()
                .as_micros()
                .min(u128::from(u64::MAX)) as u64,
            allocation_count: counters.allocation_count,
            allocated_bytes: counters.allocated_bytes,
            deallocation_count: counters.deallocation_count,
            deallocated_bytes: counters.deallocated_bytes,
            reallocation_count: counters.reallocation_count,
        };
        self.next_sample_order = self.next_sample_order.saturating_add(1);
        self.shell
            .allocations
            .record(sample.clone(), self.recent_capacity);
        let surface = self
            .surfaces
            .entry(surface_id.to_owned())
            .or_insert_with(|| SurfaceAccumulator {
                module_id: module_id.map(str::to_owned),
                scope: ScopeAccumulator::default(),
                invalidation: None,
            });
        if surface.module_id.is_none() {
            surface.module_id = module_id.map(str::to_owned);
        }
        surface
            .scope
            .allocations
            .record(sample, self.recent_capacity);
    }

    pub(crate) fn record_shell_stage(
        &mut self,
        stage: ProfilingStage,
        duration: Duration,
        trigger_kind: Option<&str>,
    ) {
        let sample = self.make_sample(stage, duration, None, None, None, trigger_kind);
        self.shell.record_stage(sample, self.recent_capacity);
    }

    pub(crate) fn record_surface_stage(
        &mut self,
        surface_id: &str,
        module_id: Option<&str>,
        stage: ProfilingStage,
        duration: Duration,
        trigger_kind: Option<&str>,
    ) {
        let module_id = module_id.filter(|id| !id.is_empty());
        let sample = self.make_sample(
            stage,
            duration,
            Some(surface_id),
            module_id,
            None,
            trigger_kind,
        );
        self.shell
            .record_stage(sample.clone(), self.recent_capacity);
        let surface = self
            .surfaces
            .entry(surface_id.to_string())
            .or_insert_with(|| SurfaceAccumulator {
                module_id: module_id.map(str::to_string),
                scope: ScopeAccumulator::default(),
                invalidation: None,
            });
        if surface.module_id.is_none() {
            surface.module_id = module_id.map(str::to_string);
        }
        surface.scope.record_stage(sample, self.recent_capacity);
    }

    pub(crate) fn record_backend_stage(
        &mut self,
        interface: &str,
        provider_id: &str,
        stage: ProfilingBackendStage,
        duration: Duration,
        trigger_kind: Option<&str>,
    ) {
        let sample = self.make_backend_sample(stage, duration, trigger_kind);
        self.backends
            .entry((interface.to_string(), provider_id.to_string()))
            .or_default()
            .stages
            .entry(stage)
            .or_default()
            .record(sample, self.recent_capacity);
    }

    pub(crate) fn record_surface_redraw(
        &mut self,
        surface_id: &str,
        module_id: Option<&str>,
        trigger_kind: Option<&str>,
    ) {
        let module_id = module_id.filter(|id| !id.is_empty());
        let sample = self.make_sample(
            ProfilingStage::RedrawCount,
            Duration::from_micros(1),
            Some(surface_id),
            module_id,
            Some(1),
            trigger_kind,
        );
        self.shell
            .record_stage(sample.clone(), self.recent_capacity);
        let surface = self
            .surfaces
            .entry(surface_id.to_string())
            .or_insert_with(|| SurfaceAccumulator {
                module_id: module_id.map(str::to_string),
                scope: ScopeAccumulator::default(),
                invalidation: None,
            });
        if surface.module_id.is_none() {
            surface.module_id = module_id.map(str::to_string);
        }
        surface.scope.record_stage(sample, self.recent_capacity);
    }

    pub(crate) fn record_surface_invalidation(
        &mut self,
        surface_id: &str,
        module_id: Option<&str>,
        invalidation: ProfilingInvalidationSnapshot,
    ) {
        let module_id = module_id.filter(|id| !id.is_empty());
        let surface = self
            .surfaces
            .entry(surface_id.to_string())
            .or_insert_with(|| SurfaceAccumulator {
                module_id: module_id.map(str::to_string),
                scope: ScopeAccumulator::default(),
                invalidation: None,
            });
        if surface.module_id.is_none() {
            surface.module_id = module_id.map(str::to_string);
        }
        surface.invalidation = Some(invalidation);
    }

    fn make_sample(
        &mut self,
        stage: ProfilingStage,
        duration: Duration,
        surface_id: Option<&str>,
        module_id: Option<&str>,
        redraw_count: Option<u32>,
        trigger_kind: Option<&str>,
    ) -> ProfilingSample {
        let sample = ProfilingSample {
            stage,
            order: self.next_sample_order,
            timestamp_micros: self
                .session_started
                .elapsed()
                .as_micros()
                .min(u128::from(u64::MAX)) as u64,
            duration_micros: duration.as_micros().min(u128::from(u64::MAX)) as u64,
            surface_id: surface_id.map(str::to_string),
            module_id: module_id.map(str::to_string),
            redraw_count,
            trigger_kind: trigger_kind.map(str::to_string),
        };
        self.next_sample_order = self.next_sample_order.saturating_add(1);
        sample
    }

    fn make_backend_sample(
        &mut self,
        stage: ProfilingBackendStage,
        duration: Duration,
        trigger_kind: Option<&str>,
    ) -> ProfilingBackendSample {
        let sample = ProfilingBackendSample {
            stage,
            order: self.next_sample_order,
            timestamp_micros: self
                .session_started
                .elapsed()
                .as_micros()
                .min(u128::from(u64::MAX)) as u64,
            duration_micros: duration.as_micros().min(u128::from(u64::MAX)) as u64,
            trigger_kind: trigger_kind.map(str::to_string),
        };
        self.next_sample_order = self.next_sample_order.saturating_add(1);
        sample
    }
}

impl Shell {
    pub(crate) fn profiling_enabled(&self) -> bool {
        self.debug.profiling_enabled
    }

    pub(crate) fn record_shell_profiling_stage(
        &mut self,
        stage: ProfilingStage,
        duration: Duration,
        trigger_kind: Option<&str>,
    ) {
        if !self.profiling_enabled() {
            return;
        }
        self.profiling
            .record_shell_stage(stage, duration, trigger_kind);
    }

    pub(crate) fn record_surface_profiling_stage(
        &mut self,
        surface_id: &str,
        module_id: Option<&str>,
        stage: ProfilingStage,
        duration: Duration,
        trigger_kind: Option<&str>,
    ) {
        if !self.profiling_enabled() {
            return;
        }
        self.profiling
            .record_surface_stage(surface_id, module_id, stage, duration, trigger_kind);
    }

    pub(crate) fn record_surface_redraw(
        &mut self,
        surface_id: &str,
        module_id: Option<&str>,
        trigger_kind: Option<&str>,
    ) {
        if !self.profiling_enabled() {
            return;
        }
        self.profiling
            .record_surface_redraw(surface_id, module_id, trigger_kind);
    }

    pub(crate) fn record_surface_invalidation(
        &mut self,
        surface_id: &str,
        module_id: Option<&str>,
        invalidation: ProfilingInvalidationSnapshot,
    ) {
        if !self.profiling_enabled() {
            return;
        }
        self.profiling
            .record_surface_invalidation(surface_id, module_id, invalidation);
    }

    pub(crate) fn record_surface_allocations(
        &mut self,
        surface_id: &str,
        module_id: Option<&str>,
        counters: AllocationCounters,
    ) {
        if !self.profiling_enabled() || !mesh_core_debug::allocation::profiling_available() {
            return;
        }
        self.profiling
            .record_surface_allocations(surface_id, module_id, counters);
    }

    pub(crate) fn record_backend_profiling_stage(
        &mut self,
        interface: &str,
        provider_id: &str,
        stage: ProfilingBackendStage,
        duration: Duration,
        trigger_kind: Option<&str>,
    ) {
        if !self.profiling_enabled() {
            return;
        }
        self.profiling
            .record_backend_stage(interface, provider_id, stage, duration, trigger_kind);
    }

    pub(crate) fn record_backend_state_publish_delivery(
        &mut self,
        interface: &str,
        provider_id: &str,
        duration: Duration,
        trigger_kind: Option<&str>,
    ) {
        self.record_backend_profiling_stage(
            interface,
            provider_id,
            ProfilingBackendStage::StatePublishDelivery,
            duration,
            trigger_kind,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_debug::{ProfilingInvalidationSnapshot, RetainedInvalidationCounts};

    #[test]
    fn perf_hud_snapshot_borrows_bounded_surface_history_and_live_counters() {
        let mut profiling = ProfilingRuntimeState::default();
        for duration in [Duration::from_micros(4_000), Duration::from_micros(18_000)] {
            profiling.record_surface_stage(
                "surface",
                Some("@mesh/test"),
                ProfilingStage::TotalSurfaceRender,
                duration,
                Some("paint"),
            );
        }
        profiling.record_surface_redraw("surface", Some("@mesh/test"), Some("present"));
        profiling.record_surface_invalidation(
            "surface",
            Some("@mesh/test"),
            ProfilingInvalidationSnapshot {
                retained_generation: 9,
                retained: RetainedInvalidationCounts {
                    style: 2,
                    state: 1,
                    ..RetainedInvalidationCounts::default()
                },
                paint: mesh_core_debug::RetainedPaintSnapshot {
                    entries_rebuilt: 2,
                    damage_rect_count: 3,
                    damage_area: 400,
                    surface_area: 4_000,
                    ..mesh_core_debug::RetainedPaintSnapshot::default()
                },
                ..ProfilingInvalidationSnapshot::default()
            },
        );

        let hud = profiling.perf_hud_snapshot("surface");
        assert_eq!(
            &hud.frame_times_micros[..hud.frame_time_count],
            &[4_000, 18_000]
        );
        assert_eq!(hud.redraw_count, 1);
        assert_eq!(hud.retained_generation, 9);
        assert_eq!(hud.dirty_nodes, 3);
        assert_eq!(hud.entries_rebuilt, 2);
        assert_eq!(hud.damage_rect_count, 3);
        assert_eq!(hud.damage_area, 400);
        assert_eq!(hud.surface_area, 4_000);
    }

    #[test]
    fn profiling_attribution_is_cumulative_bounded_and_sorted() {
        let mut profiling = ProfilingRuntimeState::default();
        for (key, micros) in [("slow", 40), ("fast", 5), ("slow", 20)] {
            profiling.record_surface_stage(
                "surface",
                Some("@mesh/test"),
                ProfilingStage::TreeBuild,
                Duration::from_micros(micros),
                Some(&format!("attribution:component_instance:{key}")),
            );
        }

        let snapshot = profiling.snapshot(1);
        let surface = &snapshot.surfaces[0];
        assert_eq!(surface.attribution.len(), 2);
        assert_eq!(surface.attribution[0].key, "component_instance:slow");
        assert_eq!(surface.attribution[0].sample_count, 2);
        assert_eq!(surface.attribution[0].total_micros, 60);
        assert_eq!(surface.attribution[0].max_micros, 40);
        assert_eq!(surface.attribution[1].key, "component_instance:fast");
        assert!(
            surface
                .stages
                .iter()
                .flat_map(|stage| &stage.recent_samples)
                .all(|sample| sample.trigger_kind.as_deref().is_some())
        );
    }

    #[test]
    fn avoided_waste_is_counted_without_polluting_stage_timings() {
        let mut profiling = ProfilingRuntimeState::default();
        for _ in 0..2 {
            profiling.record_surface_stage(
                "surface",
                Some("@mesh/test"),
                ProfilingStage::TreeBuild,
                Duration::ZERO,
                Some("waste:component_build_avoided"),
            );
        }
        profiling.record_surface_stage(
            "surface",
            Some("@mesh/test"),
            ProfilingStage::StyleRestyle,
            Duration::ZERO,
            Some("waste:empty_restyle_avoided"),
        );

        let snapshot = profiling.snapshot(1);
        let surface = &snapshot.surfaces[0];
        assert!(surface.stages.is_empty());
        assert_eq!(
            surface
                .wasted_work_avoided
                .iter()
                .map(|entry| (entry.kind.as_str(), entry.count))
                .collect::<Vec<_>>(),
            vec![("component_build_avoided", 2), ("empty_restyle_avoided", 1),]
        );
    }

    #[test]
    fn render_pass_allocations_are_aggregated_and_recent_samples_are_bounded() {
        let mut profiling = ProfilingRuntimeState {
            recent_capacity: 2,
            ..ProfilingRuntimeState::default()
        };
        for allocated_bytes in [100, 200, 300] {
            profiling.record_surface_allocations(
                "surface",
                Some("@mesh/test"),
                AllocationCounters {
                    allocation_count: 2,
                    allocated_bytes,
                    deallocation_count: 1,
                    deallocated_bytes: allocated_bytes / 2,
                    reallocation_count: 1,
                },
            );
        }

        let snapshot = profiling.snapshot(1);
        let shell = snapshot
            .shell
            .allocations
            .expect("shell allocation summary");
        let surface = snapshot.surfaces[0]
            .allocations
            .as_ref()
            .expect("surface allocation summary");
        assert_eq!(shell, *surface);
        assert_eq!(surface.sample_count, 3);
        assert_eq!(surface.allocation_count, 6);
        assert_eq!(surface.allocated_bytes, 600);
        assert_eq!(surface.deallocation_count, 3);
        assert_eq!(surface.deallocated_bytes, 300);
        assert_eq!(surface.reallocation_count, 3);
        assert_eq!(surface.max_allocated_bytes_per_pass, 300);
        assert_eq!(surface.recent_samples.len(), 2);
        assert_eq!(surface.recent_samples[0].allocated_bytes, 200);
        assert_eq!(surface.recent_samples[1].allocated_bytes, 300);
    }
}
