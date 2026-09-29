use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::*;

fn single_worker() -> JobSystem {
    JobSystem::new(JobSystemConfig {
        interactive: LaneConfig {
            workers: 1,
            compute_threads: None,
        },
        browse: LaneConfig {
            workers: 1,
            compute_threads: Some(1),
        },
        background: LaneConfig {
            workers: 1,
            compute_threads: Some(1),
        },
    })
}

/// Occupies the lane's only worker until the returned sender is used/dropped.
fn block_lane(jobs: &JobSystem, lane: Lane) -> (mpsc::Sender<()>, JobHandle<(), ()>) {
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let (started_tx, started_rx) = mpsc::channel::<()>();
    let handle = jobs.submit(
        JobSpec::new(lane, Priority::Interactive, "blocker"),
        move |_| {
            started_tx.send(()).unwrap();
            let _ = release_rx.recv();
            Ok(())
        },
    );
    started_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("blocker did not start");
    (release_tx, handle)
}

#[test]
fn runs_job_and_returns_value() {
    let jobs = single_worker();
    let h = jobs.submit(
        JobSpec::new(Lane::Interactive, Priority::Interactive, "t"),
        |_| Ok::<_, ()>(21 * 2),
    );
    assert_eq!(h.wait(), Ok(42));
}

#[test]
fn reports_failure() {
    let jobs = single_worker();
    let h = jobs.submit(
        JobSpec::new(Lane::Interactive, Priority::Interactive, "t"),
        |_| Err::<(), _>("boom"),
    );
    assert_eq!(h.wait(), Err(JobError::Failed("boom")));
}

#[test]
fn panic_is_contained_and_worker_survives() {
    let jobs = single_worker();
    let h = jobs.submit(
        JobSpec::new(Lane::Interactive, Priority::Interactive, "t"),
        |_| {
            if true {
                panic!("kaboom");
            }
            Ok::<(), ()>(())
        },
    );
    assert_eq!(h.wait(), Err(JobError::Panicked("kaboom".into())));
    let h = jobs.submit(
        JobSpec::new(Lane::Interactive, Priority::Interactive, "t"),
        |_| Ok::<_, ()>(1),
    );
    assert_eq!(h.wait(), Ok(1));
}

#[test]
fn higher_priority_runs_first_fifo_within_priority() {
    let jobs = single_worker();
    let (release, blocker) = block_lane(&jobs, Lane::Background);
    let order = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();
    for (name, prio) in [
        ("export", Priority::Export),
        ("thumb-a", Priority::VisibleThumbnail),
        ("preview", Priority::VisiblePreview),
        ("thumb-b", Priority::VisibleThumbnail),
    ] {
        let order = Arc::clone(&order);
        handles.push(
            jobs.submit(JobSpec::new(Lane::Background, prio, "t"), move |_| {
                order.lock().unwrap().push(name);
                Ok::<(), ()>(())
            }),
        );
    }
    release.send(()).unwrap();
    blocker.wait().unwrap();
    for h in handles {
        h.wait().unwrap();
    }
    assert_eq!(
        *order.lock().unwrap(),
        vec!["preview", "thumb-a", "thumb-b", "export"]
    );
}

#[test]
fn superseded_queued_job_is_skipped() {
    let jobs = single_worker();
    let (release, blocker) = block_lane(&jobs, Lane::Interactive);
    let spec = || JobSpec::new(Lane::Interactive, Priority::Interactive, "r").superseding("render");
    let first = jobs.submit(spec(), |_| Ok::<_, ()>("first"));
    let second = jobs.submit(spec(), |_| Ok::<_, ()>("second"));
    release.send(()).unwrap();
    blocker.wait().unwrap();
    assert_eq!(first.wait(), Err(JobError::Cancelled));
    assert_eq!(second.wait(), Ok("second"));
}

#[test]
fn superseding_cancels_running_job() {
    let jobs = JobSystem::new(JobSystemConfig {
        interactive: LaneConfig {
            workers: 2,
            compute_threads: None,
        },
        browse: LaneConfig {
            workers: 1,
            compute_threads: Some(1),
        },
        background: LaneConfig {
            workers: 1,
            compute_threads: Some(1),
        },
    });
    let (started_tx, started_rx) = mpsc::channel();
    let spec = || JobSpec::new(Lane::Interactive, Priority::Interactive, "r").superseding("render");
    let first = jobs.submit(spec(), move |token| {
        started_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !token.is_cancelled() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        Ok::<_, ()>("first finished")
    });
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let t0 = Instant::now();
    let second = jobs.submit(spec(), |_| Ok::<_, ()>("second"));
    assert_eq!(first.wait(), Err(JobError::Cancelled));
    assert!(
        t0.elapsed() < Duration::from_secs(1),
        "running job did not observe cancel"
    );
    assert_eq!(second.wait(), Ok("second"));
}

#[test]
fn cancel_key_cancels_without_replacement() {
    let jobs = single_worker();
    let (release, blocker) = block_lane(&jobs, Lane::Interactive);
    let h = jobs.submit(
        JobSpec::new(Lane::Interactive, Priority::Interactive, "r").superseding("k"),
        |_| Ok::<_, ()>(()),
    );
    jobs.cancel_key("k");
    release.send(()).unwrap();
    blocker.wait().unwrap();
    assert_eq!(h.wait(), Err(JobError::Cancelled));
}

#[test]
fn background_lane_does_not_block_interactive_lane() {
    let jobs = single_worker();
    let (release, blocker) = block_lane(&jobs, Lane::Background);
    let h = jobs.submit(
        JobSpec::new(Lane::Interactive, Priority::Interactive, "t"),
        |_| Ok::<_, ()>("interactive ran"),
    );
    assert_eq!(h.wait(), Ok("interactive ran"));
    release.send(()).unwrap();
    blocker.wait().unwrap();
}

#[test]
fn background_lane_uses_bounded_compute_pool() {
    let jobs = single_worker();
    let h = jobs.submit(
        JobSpec::new(Lane::Background, Priority::Export, "t"),
        |_| Ok::<_, ()>(rayon::current_num_threads()),
    );
    assert_eq!(h.wait(), Ok(1));
}

#[test]
fn ready_handle_returns_immediately() {
    let jobs = single_worker();
    let h: JobHandle<i32, ()> = JobHandle::ready(jobs.next_id(), Ok(7));
    assert_eq!(h.wait(), Ok(7));
}

#[test]
fn drop_cancels_queued_jobs() {
    let jobs = single_worker();
    let (release, blocker) = block_lane(&jobs, Lane::Interactive);
    let queued = jobs.submit(JobSpec::new(Lane::Interactive, Priority::Idle, "q"), |_| {
        Ok::<_, ()>(1)
    });
    assert_eq!(jobs.queued(Lane::Interactive), 1);
    // Release the blocker from another thread after drop begins.
    let releaser = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(20));
        let _ = release.send(());
    });
    drop(jobs);
    releaser.join().unwrap();
    let _ = blocker.wait();
    assert_eq!(queued.wait(), Err(JobError::Cancelled));
}

#[test]
fn finished_keyed_jobs_leave_the_supersede_map() {
    let jobs = single_worker();
    let handles: Vec<_> = (0..50)
        .map(|i| {
            jobs.submit(
                JobSpec::new(Lane::Browse, Priority::VisibleThumbnail, "t")
                    .superseding(format!("thumb-{i}")),
                |_| Ok::<_, ()>(()),
            )
        })
        .collect();
    for h in handles {
        h.wait().unwrap();
    }
    // A cancelled (skipped) job is removed too.
    let (release, blocker) = block_lane(&jobs, Lane::Browse);
    let queued = jobs.submit(
        JobSpec::new(Lane::Browse, Priority::VisibleThumbnail, "t").superseding("x"),
        |_| Ok::<_, ()>(()),
    );
    jobs.cancel_key("x");
    drop(release);
    blocker.wait().unwrap();
    assert_eq!(queued.wait(), Err(JobError::Cancelled));
    assert_eq!(jobs.keyed_jobs(), 0);
}

#[test]
fn a_superseding_job_keeps_its_key_when_the_older_one_finishes() {
    let jobs = single_worker();
    let (release, blocker) = block_lane(&jobs, Lane::Browse);
    let spec = || JobSpec::new(Lane::Browse, Priority::VisibleThumbnail, "t").superseding("k");
    let older = jobs.submit(spec(), |_| Ok::<_, ()>(1));
    let newer = jobs.submit(spec(), |_| Ok::<_, ()>(2));
    drop(release);
    blocker.wait().unwrap();
    assert_eq!(older.wait(), Err(JobError::Cancelled));
    // The older job's cleanup ran first and must not have removed the newer key...
    assert_eq!(newer.wait(), Ok(2));
    assert_eq!(jobs.keyed_jobs(), 0);
}

#[test]
fn browse_lane_runs_while_background_is_busy() {
    let jobs = single_worker();
    let (release, blocker) = block_lane(&jobs, Lane::Background);
    let thumb = jobs.submit(
        JobSpec::new(Lane::Browse, Priority::VisibleThumbnail, "t"),
        |_| Ok::<_, ()>("thumb"),
    );
    assert_eq!(thumb.wait(), Ok("thumb"));
    drop(release);
    blocker.wait().unwrap();
}
