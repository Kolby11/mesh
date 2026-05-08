use mesh_core_debug::{
    ProfilingSample, ProfilingScopeSnapshot, ProfilingSnapshot, ProfilingStage,
    ProfilingStageSummary, ProfilingSurfaceSnapshot,
};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::time::Duration;

const DEFAULT_RECENT_CAPACITY: usize = 16;

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
        if self.recent_samples.len() >= recent_capacity {
            self.recent_samples.pop_front();
        }
        self.recent_samples.push_back(sample);
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
    redraw_count: u64,
    total_surface_render_time_micros: u64,
}

impl ScopeAccumulator {
    fn record_stage(&mut self, sample: ProfilingSample, recent_capacity: usize) {
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
        self.stages
            .entry(sample.stage)
            .or_default()
            .record(sample, recent_capacity);
    }

    fn snapshot(&self) -> ProfilingScopeSnapshot {
        ProfilingScopeSnapshot {
            stages: self
                .stages
                .iter()
                .map(|(stage, summary)| summary.snapshot(*stage))
                .collect(),
            redraw_count: self.redraw_count,
            total_surface_render_time_micros: self.total_surface_render_time_micros,
        }
    }
}

#[derive(Debug, Default)]
struct SurfaceAccumulator {
    module_id: Option<String>,
    scope: ScopeAccumulator,
}

#[derive(Debug)]
pub(crate) struct ProfilingRuntimeState {
    session_id: u64,
    next_sample_order: u64,
    recent_capacity: usize,
    shell: ScopeAccumulator,
    surfaces: HashMap<String, SurfaceAccumulator>,
}

impl Default for ProfilingRuntimeState {
    fn default() -> Self {
        Self {
            session_id: 0,
            next_sample_order: 0,
            recent_capacity: DEFAULT_RECENT_CAPACITY,
            shell: ScopeAccumulator::default(),
            surfaces: HashMap::new(),
        }
    }
}

impl ProfilingRuntimeState {
    pub(crate) fn reset_for_new_session(&mut self, session_id: u64) {
        self.session_id = session_id;
        self.next_sample_order = 0;
        self.shell = ScopeAccumulator::default();
        self.surfaces.clear();
    }

    pub(crate) fn snapshot(&self, session_id: u64) -> ProfilingSnapshot {
        let mut surfaces: Vec<_> = self
            .surfaces
            .iter()
            .map(|(surface_id, surface)| ProfilingSurfaceSnapshot {
                surface_id: surface_id.clone(),
                module_id: surface.module_id.clone(),
                stages: surface
                    .scope
                    .snapshot()
                    .stages,
                redraw_count: surface.scope.redraw_count,
                total_surface_render_time_micros: surface.scope.total_surface_render_time_micros,
            })
            .collect();
        surfaces.sort_by(|a, b| a.surface_id.cmp(&b.surface_id));

        ProfilingSnapshot {
            session_id: session_id.max(self.session_id),
            shell: self.shell.snapshot(),
            surfaces,
        }
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
            });
        if surface.module_id.is_none() {
            surface.module_id = module_id.map(str::to_string);
        }
        surface.scope.record_stage(sample, self.recent_capacity);
    }

    pub(crate) fn record_surface_redraw(
        &mut self,
        surface_id: &str,
        module_id: Option<&str>,
        trigger_kind: Option<&str>,
    ) {
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
            });
        if surface.module_id.is_none() {
            surface.module_id = module_id.map(str::to_string);
        }
        surface.scope.record_stage(sample, self.recent_capacity);
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
            duration_micros: duration.as_micros().min(u128::from(u64::MAX)) as u64,
            surface_id: surface_id.map(str::to_string),
            module_id: module_id.map(str::to_string),
            redraw_count,
            trigger_kind: trigger_kind.map(str::to_string),
        };
        self.next_sample_order = self.next_sample_order.saturating_add(1);
        sample
    }
}
