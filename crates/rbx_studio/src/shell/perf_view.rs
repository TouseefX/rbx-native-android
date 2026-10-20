//! The Performance view: the viewport tinted by what each part costs, one
//! of `rbx_viewer::perf`'s five measures at a time. Studio only profiles a
//! running game (the MicroProfiler); this answers "what here is expensive"
//! while the place is still being edited, and never writes to it.
//!
//! The figures come from a thread of its own, started when the view is
//! turned on and dropped with it: it takes one copy of the DOM, then follows
//! the editor's edits through the same snapshot-and-`Change` log the render
//! thread is sent (see `workspace_view::pump`'s `Command::Changes`), so an
//! edit costs a re-read of the parts it names, not of the place. Assets are
//! read from the local cache only; one still downloading is looked for again
//! every couple of seconds.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use gpui_kit::*;
use rbx_assets::{AssetCache, AssetRef};
use rbx_dom::{Change, Ref, Snapshot, Variant, WeakDom};
use rbx_viewer::perf::{Metric, Metrics, Report};

use super::Shell;

/// How long the thread waits for an edit before looking for assets that had
/// not downloaded yet.
const RETRY: Duration = Duration::from_secs(2);
/// How often the UI side collects a finished report.
const POLL: Duration = Duration::from_millis(100);

enum Job {
    Changes(Vec<Snapshot>, Vec<Change>),
    Metric(Metric),
}

/// The view while it is on.
pub(super) struct PerfView {
    pub(super) metric: Metric,
    /// The latest figures, `None` until the first report lands.
    pub(super) report: Option<Arc<Report>>,
    /// Dropping it ends the thread, at its next look at the queue, not
    /// joined, so turning the view off mid-measure never waits on it.
    jobs: Sender<Job>,
    reports: Receiver<Report>,
}

impl PerfView {
    fn spawn(dom: WeakDom, metric: Metric) -> Self {
        let (jobs, inbox) = mpsc::channel();
        let (outbox, reports) = mpsc::channel();
        if let Err(err) = thread::Builder::new()
            .name("rbxstudio-perf".to_string())
            .spawn(move || run(dom, metric, &inbox, &outbox))
        {
            eprintln!("rbxstudio: no performance view thread could be started: {err}");
        }
        PerfView {
            metric,
            report: None,
            jobs,
            reports,
        }
    }
}

fn run(mut dom: WeakDom, mut metric: Metric, inbox: &Receiver<Job>, outbox: &Sender<Report>) {
    let cache = AssetCache::new(None).ok();
    let mut fetch = |asset: &AssetRef| match (asset, &cache) {
        (AssetRef::Id(id), Some(cache)) => cache.get_id(*id),
        (AssetRef::Native(path), Some(cache)) => cache.get_native(path),
        _ => None,
    };
    let started = Instant::now();
    let mut metrics = Metrics::new(&dom, &mut fetch);
    eprintln!(
        "rbxstudio: performance view measured {} parts in {:.1} ms",
        metrics.len(),
        started.elapsed().as_secs_f64() * 1000.0
    );
    let mut owed = true;
    loop {
        if owed {
            owed = false;
            if outbox.send(metrics.report(metric)).is_err() {
                return;
            }
        }
        let first = match inbox.recv_timeout(RETRY) {
            Ok(job) => job,
            Err(RecvTimeoutError::Timeout) => {
                owed = metrics.retry(&mut fetch);
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => return,
        };
        // Everything already queued in one go: a drag sends an edit per
        // mouse move, and one report after the last of them is all that
        // shows.
        for job in std::iter::once(first).chain(inbox.try_iter()) {
            match job {
                Job::Changes(snapshots, changes) => {
                    dom.mirror(snapshots);
                    metrics.patch(&dom, &changes, &mut fetch);
                }
                Job::Metric(next) => metric = next,
            }
            owed = true;
        }
    }
}

impl Shell {
    /// Turns the Performance view on with `metric`, switches it to
    /// another, or turns it off with `None`. Edit only: the live game's DOM
    /// shown during Play is not the place being edited.
    pub(super) fn set_perf_metric(&mut self, metric: Option<Metric>, cx: &mut Context<Self>) {
        match (metric, self.perf.as_mut()) {
            (None, _) => {
                if self.perf.take().is_some() {
                    self.viewport.update(cx, |viewport, cx| {
                        viewport.set_heat(Vec::new());
                        viewport.set_heat_readout(None, cx);
                    });
                }
            }
            (Some(metric), Some(perf)) => {
                perf.metric = metric;
                let _ = perf.jobs.send(Job::Metric(metric));
            }
            (Some(metric), None) => {
                if self.live_shown() {
                    return;
                }
                self.perf = Some(PerfView::spawn(self.dom.clone(), metric));
                cx.spawn(async move |shell, cx| loop {
                    cx.background_executor().timer(POLL).await;
                    let on = shell.update(cx, |shell, cx| shell.perf_poll(cx));
                    if !matches!(on, Ok(true)) {
                        return;
                    }
                })
                .detach();
            }
        }
        cx.notify();
    }

    /// Hands the latest report, if one landed, to the viewport and the dock.
    /// `false` once the view is off, which ends the polling.
    fn perf_poll(&mut self, cx: &mut Context<Self>) -> bool {
        if self.live_shown() {
            self.set_perf_metric(None, cx);
        }
        let Some(perf) = self.perf.as_mut() else {
            return false;
        };
        let Some(report) = perf.reports.try_iter().last() else {
            return true;
        };
        let bins = report.bins.clone();
        perf.report = Some(Arc::new(report));
        self.viewport
            .update(cx, |viewport, _| viewport.set_heat(bins));
        cx.notify();
        true
    }

    /// Passes one edit on to the Performance view, if it is on.
    pub(super) fn perf_changes(&self, snapshots: &[Snapshot], changes: &[Change]) {
        if let Some(perf) = self.perf.as_ref().filter(|_| !self.live_shown()) {
            let _ = perf
                .jobs
                .send(Job::Changes(snapshots.to_vec(), changes.to_vec()));
        }
    }

    /// The line the viewport shows beside the cursor over `part`.
    pub(super) fn perf_readout(&self, part: Option<Ref>) -> Option<SharedString> {
        let perf = self.perf.as_ref()?;
        let readout = perf.report.as_ref()?.readout(part?)?;
        Some(SharedString::from(format!(
            "{} · {} — {}",
            readout.name,
            perf.metric.format(readout.value),
            readout.detail
        )))
    }

    /// Selects `part` and puts the camera on it: a click on the dock's
    /// worst-parts list.
    pub(super) fn perf_focus(&mut self, part: Ref, cx: &mut Context<Self>) {
        self.select(part, cx);
        let Some(properties) = self.dom.get(part).map(|p| p.properties()) else {
            return;
        };
        let centre = match properties.get("CFrame") {
            Some(Variant::CFrame(cframe)) => {
                glam::Vec3::new(cframe.position.x, cframe.position.y, cframe.position.z)
            }
            _ => return,
        };
        let reach = match properties.get("size") {
            Some(Variant::Vector3(size)) => glam::Vec3::new(size.x, size.y, size.z).length() * 0.5,
            _ => 2.0,
        }
        .max(2.0);
        let eye = centre + glam::Vec3::new(-0.55, 0.45, -1.0).normalize() * reach * 3.0;
        self.viewport
            .read(cx)
            .look_at(eye.to_array(), centre.to_array());
    }
}
